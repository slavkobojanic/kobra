# Shop: a workspace of services with paybox

`examples/shop` is a production-style shop built as a [workspace](../apps/workspaces.md) of two apps and
the library between them:

- **shop** is the storefront. Customers browse products, check out as guests and pay by card. A
  background job emails the confirmation over SMTP, and staff sign in to an admin page.
- **paybox** is a fake card payment provider, a server-only K app with its own database. It charges
  test card numbers, refunds, and tells each merchant about its payments by calling the merchant back
  from retried background jobs.
- **pay-contracts** is a library with the two protocols between them, `Payments` (served by paybox)
  and `PaymentEvents` (served by the shop), and the types they carry.

Everything runs locally with no account on any service: `docker compose up` starts Postgres, Mailpit
(an SMTP server with a web inbox), paybox, two shop replicas and a reverse proxy, and the same system
runs on a local kind cluster. [Deployment](../apps/deployment.md) covers those parts; this chapter
follows a purchase through the code.

## How a purchase flows

```text
 browser ──▶ shop (2 replicas, behind Caddy) ──Payments──▶ paybox ──▶ Postgres (paybox)
    ▲              │  ▲                                       │
    │              │  └──────────PaymentEvents (job)──────────┘
    │              ▼
    │           Postgres (shop) ──job──▶ Mailpit (SMTP)
    └── receipt polls until paid
```

1. The product page submits an `@Form` of `OrderRequest`. `StoreApi` reserves stock and records the
   order in one transaction, then asks the injected `PaymentGateway` where to send the customer.
2. In production and development the gateway is `PayboxCheckout`: it registers the shop with paybox
   and sends the customer to the shop's own card form.
3. The card form calls the shop's `CardProtocol.pay`. The shop's **server** charges the order through
   paybox; the browser never talks to paybox, because a remote client holds a service token.
4. Paybox records the charge and, in the same transaction, enqueues `EventDeliveryJob`, which calls the
   shop's `PaymentEvents.chargeSucceeded` back, retrying while the shop is unreachable.
5. `PaymentEventsApi` hands the charge to `PaymentRecorder`, which marks the order paid and enqueues
   `OrderConfirmationJob` in the same transaction. The job emails the customer.
6. The receipt page polls with `reload()` until the payment is confirmed.

## Running it

```bash
cd examples/shop
k dev                                      # paybox on 8080, the shop on 8081
```

That is the whole setup. Open `http://localhost:8081` and pay with `4242 4242 4242 4242`. `k dev` at
the workspace root builds and serves both apps, wires the shop's `PAYBOX_URL` and `PAYBOX_TOKEN` to the
paybox it started, and gives both one development secret, so the services trust each other. Each member's
`[dev]` section in its `k.toml` supplies the rest
([project files](../reference/project-files.md#development-settings)): the shop's address for paybox's
callbacks, a generated admin password (printed at startup, for `/sign-in` as `admin@shop.example`), and
the demo catalog, loaded when the shop's SQLite database is created. Confirmation emails land in `k dev`'s
own mail catcher, which prints each one with its links and keeps it in `shop/.k/mail/`.

```bash
k test           # every member's tests, against in-memory SQLite
k check          # types, layering, every lockfile and the tests, in every member
```

## The contract library

```k
// examples/shop/pay-contracts/src/pay/Payments.k
public protocol Payments requires ServiceAuthenticated {
    // ...
    rpc registerMerchant(callbackUrl: Url) throws RegistrationError -> Merchant
    // ...
    rpc charge(request: ChargeRequest) throws ChargeError -> Charge
    // ...
    rpc refund(request: RefundRequest) throws RefundError -> Refund
}
```

`ServiceAuthenticated` admits only callers holding a service token signed with the shared secret, so no
browser can call it. Each app records the protocols it serves in `rpc.lock`; a change that would break
the deployed caller fails `k check` unless it is marked `@Breaking`.

## The shop's app declaration

```k
// examples/shop/shop/App.k
@Layout(Storefront)
@SignInPage(.signIn)
public app Shop {
    use PayboxCheckout for PaymentGateway in .production, .development
    use StripeDisabled for StripeEvents
    use LocalCheckout for PaymentGateway in .test
    use remote(PayboxService) for Payments
    // ...
}
```

- The gateway is chosen per environment; tests use `LocalCheckout`, which pays at once.
- `use remote(PayboxService) for Payments` binds the library protocol to paybox, at the address and
  with the token the `PayboxService` config reads from the environment.
- Stripe stays available as an optional binding: `StripeCheckout` and `StripeWebhookVerifier` are
  written and tested, and `StripeDisabled` is bound by default.

## Placing an order

```k
// examples/shop/shop/src/store/StoreApi.k
rpc placeOrder(request: OrderRequest) async throws CheckoutError -> PlacedOrder {
    guard let product = await catalog.bySlug(request.productSlug) else {
        throw CheckoutError.productNotFound
    }

    let order = Order(
        id: Uuid.random(),
        product: product,
        email: request.email,
        quantity: request.quantity,
        unitPrice: product.price,
        total: product.price * request.quantity,
        placedAt: clock.now
    )

    try await database.transaction {
        guard await catalog.reserve(id: product.id, quantity: order.quantity) else {
            throw CheckoutError.outOfStock(available: product.stock)
        }

        await orders.insert(order)
    }
    // ...
```

`catalog.reserve` is a conditional bulk `update`, so the last unit can't be sold twice, and the
`throw` inside the transaction rolls the order back ([Persistence](../apps/persistence.md)).

## Paybox calls the shop back

Paybox is an app with no pages and no rules in its `App.k`:

```k
// examples/shop/paybox/App.k
public app Paybox {}
```

Merchants register their callback URLs at run time, so paybox builds a client per call with
`RemoteClient.at` ([Workspaces and services](../apps/workspaces.md)), from a job that retries with
backoff:

```k
// examples/shop/paybox/src/payments/EventDeliveryJob.k
@Retry(attempts: 12, backoff: 2.seconds)
@JobName("payment-event")
service EventDeliveryJob implements Job<EventDelivery> {
    @Inject let merchants: MerchantRepository
    @Inject let charges: ChargeRepository
    @Inject let refunds: RefundRepository
    @Inject let notifier: MerchantNotifier

    func run(payload: EventDelivery) async throws JobError {
        guard let merchant = await merchants.byName(payload.merchant) else {
            throw JobError.permanent(reason: "No merchant is called {payload.merchant}")
        }
        // ...
```

## The shop records the payment once

```k
// examples/shop/shop/src/paybox/PaymentEventsApi.k
public service PaymentEventsApi implements PaymentEvents {
    @Inject let recorder: PaymentRecorder

    rpc chargeSucceeded(charge: Charge) async throws RpcOnlyError {
        guard let orderId = Uuid.parse(charge.reference) else {
            Log.error("Paybox reported charge {charge.id} for {charge.reference}, which isn't an order of this shop")

            return
        }

        let outcome = await recorder.recordPayment(orderId: orderId, amount: charge.amount, reference: charge.id)
        // ...
```

`PaymentRecorder` marks the order paid with a conditional update and enqueues the confirmation in the
same transaction, so a repeated event changes nothing and the email is sent once per paid order
([Background jobs](../apps/jobs.md)).

## Test cards

Paybox never moves money. Like a provider's test mode, the card number decides what happens:

| Card number | Outcome |
|---|---|
| `4242 4242 4242 4242` | the charge succeeds |
| `4000 0000 0000 0002` | declined: card declined |
| `4000 0000 0000 9995` | declined: insufficient funds |
| `4000 0000 0000 0069` | declined: expired card |
| any other number of 12 to 19 digits that passes the Luhn check | the charge succeeds |
| anything else | not a card number |

## The tests

Tests never touch the network, a mail server or the environment. In the shop's tests `Payments` is a
`TestRemote` whose every call is offline; tests that need answers bind `testing.FakePaybox`. Paybox's
tests call `PaymentsApi` as the merchant `shop` and deliver events to an in-process recording merchant.
The shop's webhook tests are HTTP-level: they post signed and unsigned deliveries through the real
server stack with `TestClient` ([Testing](../apps/testing.md#integration-tests-and-the-database)).

For the deployment side (the Dockerfile, Compose, Caddy, health checks, secrets and the kind
manifests) see [Deployment](../apps/deployment.md#a-system-of-services).
