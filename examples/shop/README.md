# K Shop

A small, production-style shop written entirely in Kobra, as a **workspace of two services** and the contract
library between them ([`docs/design/40-workspaces-and-services.md`](../../docs/design/40-workspaces-and-services.md)):

- **shop** is the storefront. Customers browse products, check out as guests and pay by card. A
  background job emails a confirmation over SMTP, and staff sign in to an admin page that lists orders.
- **paybox** is a fake card payment provider, a server-only Kobra app. It charges test card numbers, refunds,
  keeps merchants' callback addresses, and tells each merchant about its payments by calling the
  merchant back from retried background jobs. It has its own database and its own tests.
- **pay-contracts** is a library holding the two protocols between them, `Payments` (served by paybox)
  and `PaymentEvents` (served by the shop), and the types they carry.

Everything runs locally with **no account on any service**: `docker compose up` starts Postgres, Mailpit
(an SMTP server with a web inbox), paybox, two shop replicas and a reverse proxy, and the same system
runs on a local [kind](https://kind.sigs.k8s.io) cluster with the manifests in
[`deploy/kubernetes`](deploy/kubernetes). Stripe stays available as an optional binding, off by default.

## How a purchase flows

```text
 browser ──▶ shop (2 replicas, behind Caddy) ──Payments──▶ paybox ──▶ Postgres (paybox)
    ▲              │  ▲                                       │
    │              │  └──────────PaymentEvents (job)──────────┘
    │              ▼
    │           Postgres (shop) ──job──▶ Mailpit (SMTP)
    └── receipt polls until paid
```

1. `ProductDetail` submits an `@Form` of `OrderRequest`; `StoreApi` reserves stock and records the order
   in one transaction (a conditional `UPDATE`, so the last unit can't be sold twice), then asks the
   `PaymentGateway` for the payment URL.
2. In production and development the gateway is `PayboxCheckout`. It registers the shop with paybox
   (`Payments.registerMerchant`, idempotent, giving the address where paybox calls the shop back) and
   sends the customer to the shop's own card form, `/payment/<order>`. If paybox is unreachable, the
   order is cancelled and its stock released.
3. The card form calls the shop's `CardProtocol.pay`. The shop's **server** charges the order's own total
   through paybox: the browser never talks to paybox, because a remote client holds a service token. The
   order id is the charge's reference, so paying twice charges once.
4. Paybox decides by card number (below), records the charge and, in the same transaction, enqueues
   `EventDeliveryJob`. The job calls the shop's `PaymentEvents.chargeSucceeded` at the registered
   callback URL with `RemoteClient.at`, retrying with backoff while the shop is unreachable.
5. The shop's `PaymentEventsApi` hands the charge to `PaymentRecorder`, which marks the order paid with a
   conditional update and enqueues `OrderConfirmationJob` in the same transaction; a repeated event
   changes nothing. The job emails the customer through SMTP.
6. The customer's `Receipt` polls with `reload()` until the payment is confirmed.

### Test cards

Paybox is a stand-in for a payment provider, so it never moves money. Like a provider's test mode, the
card number decides what happens (any spaces or dashes are ignored):

| Card number | Outcome |
|---|---|
| `4242 4242 4242 4242` | the charge succeeds |
| `4000 0000 0000 0002` | declined: card declined |
| `4000 0000 0000 9995` | declined: insufficient funds |
| `4000 0000 0000 0069` | declined: expired card |
| any other number of 12 to 19 digits that passes the Luhn check | the charge succeeds |
| anything else | not a card number |

## Structure

```
k.toml                        [workspace] members = ["pay-contracts", "paybox", "shop"]
Dockerfile                    builds the toolchain, checks and tests every member, ships one app (APP=paybox|shop)
Dockerfile.dockerignore       the build context is the repository; only sources go in
docker-compose.yml            Postgres, Mailpit, paybox, 2 shop replicas, Caddy
docker-compose.test.yml       a throwaway Postgres for the compiler's test suite
deploy/caddy/Caddyfile        the reverse proxy in front of the shop replicas
deploy/postgres/              creates paybox's own database next to the shop's
deploy/kubernetes/            kind cluster, manifests, Secrets script and walkthrough
scripts/init-secrets.sh       generates every secret into secrets/ (git-ignored), for Compose and Kubernetes
scripts/e2e.sh                builds and starts the Compose stack, checks it over HTTP, tears it down
secrets/*.example             placeholders; real secrets are never committed

pay-contracts/                library (no App.k)
  src/pay/                    Payments, PaymentEvents, ChargeRequest, CardDetails, Charge, Refund,
                              Merchant, and their errors: ChargeError, DeclineReason, RefundError,
                              RegistrationError

paybox/                       app: serves Payments, calls each merchant's PaymentEvents
  App.k                       no rules to choose: merchants' addresses are known only at run time
  rpc.lock, schema.lock,      generated lockfiles: Payments' shape (which deployed shops rely on),
  storage.lock                the tables, the delivery job's payload
  migrations/                 0001 creates merchant_account, charge_record, refund_record
  src/merchants/              MerchantAccount, MerchantRepository, CallingMerchant (who is calling)
  src/payments/               PaymentsApi, TestCards, charges and refunds, EventDeliveryJob,
                              MerchantNotifier and RemoteNotifier (RemoteClient.at), CallbackConfig
  src/ops/HealthCheck.k       GET /health
  src/testing/                test-only: TestMerchant, RecordingMerchant, samples

shop/                         app: serves StoreProtocol, AdminProtocol, CardProtocol, PaymentEvents
  App.k                       use remote(PayboxService) for Payments; the gateway per environment;
                              the optional Stripe binding (off)
  rpc.lock, schema.lock,      generated lockfiles
  storage.lock
  migrations/                 0001 creates the catalog and orders
  scripts/demo-catalog.sql    the three demo products
  src/
    store/                    catalog and orders: entities, repositories, StoreProtocol, StoreApi,
                              StoreService, PaymentRecorder, OrderConfirmationJob
    payments/                 PaymentGateway, LocalCheckout (tests), and the optional Stripe binding:
                              StripeCheckout, StripeWebhookVerifier, StripeDisabled (the default)
    paybox/                   paying through paybox: PayboxCheckout, the Payment page, CardProtocol,
                              CardApi, CardService, PaymentEventsApi, PayboxService, PayboxCallbackConfig
    admin/                    staff sign-in and the order list
    cart/                     the cart: CartService (local storage, kept in step across tabs with
                              TabMessaging), CartBadge in the header, AddToCart on a product page
    webhooks/StripeWebhook.k  the endpoint Stripe calls when Stripe is enabled (404 otherwise)
    ops/HealthCheck.k         GET /health
    pages/                    Storefront layout, Home, ProductDetail, Receipt, SignIn, Admin, ProductCard
    testing/                  test-only: FakePaybox, the Stripe event builder, shared samples
```

Every file holds at most one public declaration, named after the file; helpers in the same file are
module-private. Each directory is a module.

## Running it

Every command below runs at the workspace root unless it says otherwise. They need the `k` toolchain;
the Docker image builds its own from this repository.

### Development

```bash
k dev
```

That one command is the whole setup: no `.env`, no secrets to generate, no Docker. Open
http://localhost:8081 and pay with `4242 4242 4242 4242`.

- **Both apps run,** paybox on 8080 and the shop on 8081 (`--port 9000` for 9000 and 9001), each line of
  their output prefixed with the app's name. `k dev` wires `PAYBOX_URL` and `PAYBOX_TOKEN` to the paybox
  it started and gives both apps one development secret generated for the run, so the services trust
  each other. Paybox calls the shop back at its own development address (`PAYBOX_CALLBACK_URL` in
  `shop/k.toml`'s `[dev.env]`).
- **Each app has its own SQLite database** below its own directory (`paybox/.k/paybox.db`,
  `shop/.k/shop.db`), created at the first start; the shop's is seeded with the demo catalog then
  (`[dev] seed`).
- **Confirmation emails go to the built-in mail catcher:** each one is printed in the shop's output with
  its links, and kept as an `.eml` file in `shop/.k/mail/`. To use Mailpit's web inbox instead, run
  `docker compose up --detach mail` and set the `SMTP_*` lines of `shop/.env.example` in `shop/.env`.
- **Sign in at http://localhost:8081/sign-in** as `admin@shop.example` with the development password
  `k dev` prints at startup (`note: SHOP_ADMIN_PASSWORD_HASH is the hash of the development password …`);
  it is generated on the first run and kept in `shop/.k/secrets/`. Signing out of the dashboard returns
  to the sign-in page.
- A change anywhere, a member's `.env` included, rebuilds and restarts both. Ctrl-C stops both.
- `k run` does the same without watching for changes.

To run one app alone, run `k dev` in `paybox/`, then `k dev --port 8081` in `shop/` with the lines of
`shop/.env.example` that point it at that paybox.

### Tests

```bash
k test           # every member's *.test.k files, against in-memory SQLite
k check          # types, layering, every lockfile and the tests, in every member
```

Tests never touch the network, a mail server or the environment. In the shop's tests `Payments` is a
`TestRemote` whose every call is offline; the tests that need answers bind `testing.FakePaybox`. Paybox's
tests call `PaymentsApi` as the merchant `shop` (`TestMerchant`) and deliver events to an in-process
`RecordingMerchant`.

`k check` also compares each app's protocols with its committed `rpc.lock`: a change that would break a
deployed caller (a removed field or error case, a changed type) fails unless it is marked
`@Breaking("…")`. `k build` updates the locks.

### Docker Compose

```bash
scripts/init-secrets.sh
docker compose up --build --wait
docker compose exec -T db psql --username=shop --dbname=shop < shop/scripts/demo-catalog.sql
```

Then open http://localhost:8080 (the shop, through Caddy) and http://localhost:8025 (the inbox), and sign
in as `admin@shop.example` with the password `init-secrets.sh` printed (it is also in
`secrets/admin_password.txt`). The script hashes it with `k hash-password` (or the `argon2` tool, or
`argon2` in a throwaway container), and fails, writing no hash, when none of them is available.

- The **Dockerfile** builds the `k` toolchain from this repository (no published image or registry
  account is needed; pass `--build-arg K_TOOLCHAIN=ghcr.io/slavkobojanic/kobra:0.1.0-rc.2` to use a published one), runs
  `k check` (which runs the tests) at the workspace root, and copies only `dist/<app>/app` into
  `gcr.io/distroless/cc`. The same Dockerfile builds both images, `k-paybox:local` and `k-shop:local`,
  with `APP=paybox` or `APP=shop`. The first build compiles the toolchain and takes a few minutes; later
  ones reuse it.
- **Postgres** holds two databases, `shop` and `paybox`, each owned by its own role;
  `deploy/postgres/create-paybox-database.sh` creates paybox's when the volume is new.
- **Two shop replicas** sit behind **Caddy**, which balances between them and takes a replica out of
  rotation while its `/health` fails. Sessions, rate limits and the job queue are in Postgres, so any
  replica can answer any request. Paybox calls the shop back at `http://shop:8080`, any replica.
- **Health checks** use `dist/app probe http://127.0.0.1:8080/health`: the images have no shell or curl.
- **Secrets** are Docker secrets from the files in `secrets/`, read through `NAME_FILE` variables. Each
  service has its own service secret: paybox verifies the shop's calls with `paybox_service_token` (the
  shop signs with it as `PAYBOX_TOKEN`), and the shop verifies paybox's callbacks with
  `shop_service_token` (paybox signs with it as `PAYBOX_CALLBACK_TOKEN`).

`scripts/e2e.sh` does all of this with secrets generated for the run, then checks the storefront and SEO,
a declined card, a paid order confirmed by paybox's event, exactly one confirmation email in Mailpit,
the admin guard, a session shared by the replicas and the sign-in rate limit, and tears the stack down.
`K_COMPOSE=1 cargo test -p k_cli --test it -- compose::shop` runs the same stack from the compiler's test
suite on test ports, with secrets from `init-secrets.sh` in a scratch directory: an order paid through
paybox and emailed to Mailpit, and, with Chrome, a mug added to the cart and bought
([testing.md §8.6](../../docs/compiler/testing.md#86-compose-smoke-tests)).

### Kubernetes

[`deploy/kubernetes/README.md`](deploy/kubernetes/README.md) walks through running the same system on a
local kind cluster: Postgres as a StatefulSet, Mailpit, paybox and two shop pods with probes, a migration
Job per app, Secrets created from `secrets/` by a script, and an Ingress (or `kubectl port-forward`).

`cargo test -p k_cli --test it -- deploy::` checks, without Docker or a cluster, that `docker-compose.yml` and the
Kubernetes manifests give every container each variable its `dist/app config` lists, every secret as a
mounted file, and the two services' tokens in the right pairs.

## Configuration

Values come from environment variables. A secret can also come from a file named by `NAME_FILE`, which is
how Docker and Kubernetes secrets arrive. `dist/<app>/app config` lists every variable a build reads, and
`dist/app` refuses to start, listing every problem, when one is missing or malformed. The tables describe
production; under `k dev` the workspace wiring, each member's `[dev]` section and the development defaults
(SQLite in `.k/`, the mail catcher) fill in everything.

**paybox**

| Variable | Required | Read by |
|---|---|---|
| `DATABASE_URL`, `DATABASE_PASSWORD[_FILE]` | yes | stdlib `DatabaseConfig` |
| `K_SERVICE_TOKEN[_FILE]` (secret) | yes | stdlib `ServiceConfig`: verifies the merchants' calls |
| `PAYBOX_CALLBACK_TOKEN[_FILE]` (secret) | no (`K_SERVICE_TOKEN`) | `payments.CallbackConfig`: signs the calls to merchants; the shop's `K_SERVICE_TOKEN` |
| `PAYBOX_CALLBACK_TIMEOUT` | no (`5s`) | `payments.CallbackConfig` |
| `K_PUBLIC_ORIGIN`, `PORT` | no | stdlib `ServerConfig` |

**shop**

| Variable | Required | Read by |
|---|---|---|
| `K_PUBLIC_ORIGIN` | no (`http://localhost:{PORT}`), but set it in every deployment | stdlib `ServerConfig`: canonical links and `PublicUrls` (payment page, email links) |
| `PORT`, `K_TRUSTED_PROXIES` | no | stdlib `ServerConfig` |
| `DATABASE_URL`, `DATABASE_PASSWORD[_FILE]` | yes | stdlib `DatabaseConfig` |
| `SMTP_HOST`, `SMTP_PORT`, `SMTP_SECURITY`, `MAIL_FROM` | yes | stdlib `SmtpConfig` |
| `K_SERVICE_TOKEN[_FILE]` (secret) | yes | stdlib `ServiceConfig`: verifies paybox's callbacks |
| `PAYBOX_URL`, `PAYBOX_TOKEN[_FILE]` (secret), `PAYBOX_TIMEOUT` | yes | `paybox.PayboxService`, the `remote` rule of `Payments` |
| `PAYBOX_CALLBACK_URL` | yes | `paybox.PayboxCallbackConfig`: the shop's address as paybox reaches it |
| `SHOP_ADMIN_EMAIL`, `SHOP_ADMIN_PASSWORD_HASH[_FILE]` (secret) | yes | `admin.AdminConfig` |

The committed `secrets/*.example` files hold placeholders, never working values; in particular
`admin_password_hash.txt.example` isn't a hash, so the shop refuses to start with it.
`scripts/init-secrets.sh` writes random values and an Argon2id hash of a random admin password (with
`k hash-password`, the `argon2` command-line tool, or `argon2` in a throwaway `alpine` container). Every
startup checks the hash against `AdminConfig`'s `@Matches`, so an empty or placeholder hash stops the shop
with exit 78. In development none of this is needed: `shop/k.toml`'s `[dev]` section generates a
development password and its hash, and gives the other development values; `dist/app` never reads it.

## Paying through Stripe instead (optional)

`StripeCheckout`, `StripeWebhookVerifier` and their tests stay in `shop/src/payments` as an optional
binding, **off by default**: `StripeDisabled` makes `/webhooks/stripe` answer `404`, and no Stripe
variable is read. To take payments through Stripe in production, replace the first two `use` rules in
`shop/App.k` with the four its documentation shows (`use StripeCheckout for PaymentGateway in
.production`, …), then set `STRIPE_API_KEY[_FILE]`, `STRIPE_WEBHOOK_SECRET[_FILE]` (see
`secrets/stripe_*.txt.example`) and register `https://<your origin>/webhooks/stripe` in the Stripe
dashboard. `STRIPE_API_URL` points `StripeCheckout` at a stand-in such as `stripe/stripe-mock` for local
runs. The card page and paybox's events stay part of the shop, so the `PAYBOX_*` variables (except
`PAYBOX_CALLBACK_URL`) are still read. `crates/k_server/tests/shop_production.rs` builds exactly that
edit and runs a Stripe checkout and its webhook against a fake Stripe.

## Language features by file

| File | Shows |
|---|---|
| `k.toml`, `*/k.toml` | a `[workspace]`, a library, `{ workspace = true }` dependencies, `[dev]` development values, generated secrets and a seed |
| `pay-contracts/src/pay/Payments.k` | a library protocol `requires ServiceAuthenticated`, documented like an API |
| `shop/App.k` | `use remote(PayboxService) for Payments`, `use … for … in .production`, `@Layout`, `@SignInPage`, app `metadata` |
| `shop/src/paybox/PayboxService.k` | a `config` that `implements RemoteService` |
| `shop/src/paybox/CardApi.k` | a server-only remote client, typed library errors mapped to the shop's own |
| `shop/src/paybox/PaymentEventsApi.k` | serving a library protocol to another service, idempotently |
| `shop/src/paybox/Payment.k` | `@Load`, `@Form` with a `@NotBlank` message, `Router.navigate(to:)`, handling `.declined(let message)` |
| `paybox/src/payments/RemoteNotifier.k` | `RemoteClient.at(url:token:timeout:)` typed by its declaration |
| `paybox/src/payments/EventDeliveryJob.k` | `Job<Payload>`, `@Retry`, `@JobName`, `JobError.retryable` and `.permanent` |
| `paybox/src/payments/PaymentsApi.k` | `ServiceCaller` through an interface seam, idempotency through a unique key, a transaction that records and enqueues |
| `paybox/src/payments/TestCards.k` | a dependency-free struct of `static func`s, the Luhn check |
| `paybox/src/payments/CallbackConfig.k` | an optional `Secret<String>?` with a fallback |
| `shop/src/store/*` | `entity`, `repository … for`, conditional updates, transactions, `Validatable`, `RateLimit` |
| `shop/src/store/OrderConfirmationJob.k` | `EmailProvider`, `PublicUrls` |
| `shop/src/payments/StripeEvents.k`, `StripeDisabled.k` | an optional integration switched by a `use` rule |
| `shop/src/payments/StripeSignature.k` | `Bytes.parse(hex:)`, async `Hmac` keyed by a `Secret`, constant-time `Mac.matches` |
| `shop/src/admin/AdminApi.k` | `SessionManager.signIn`, `Password.verify` against a `Secret` hash |
| `*/src/ops/HealthCheck.k`, `shop/src/webhooks/StripeWebhook.k` | endpoint pages with fixed URLs, `@Get`/`@Post` handlers calling the page's own functions |
| `shop/src/cart/CartService.k` | `TabMessaging` and a `TabChannel<CartChange>`: the cart stays in step across the shop's open tabs; `LocalStorage` with a `StorageKey` recorded in `storage.lock` |
| `shop/src/cart/CartBadge.k`, `AddToCart.k` | `@Rendering(.client)` components with a `loading` block, for state that lives in the browser |
| `shop/src/pages/*` | pages, a layout, a component, `metadata` with `canonical` and `structuredData`, `failed` with `status(.notFound)` |
| `*.test.k` | `test` groups, `use` overrides (fakes of a library protocol, configs, interfaces), `expect throws`, `InlineJobQueue`, `RecordingEmailProvider`, `TestClient`, `render` |

## Design documents used

- [Workspaces and services](../../docs/design/40-workspaces-and-services.md),
  [deployment](../../docs/design/38-deployment.md),
  [config and secrets](../../docs/design/25-config-and-secrets.md),
  [app structure](../../docs/design/23-app-structure.md),
  [projects and `k.toml`](../../docs/design/24-projects.md).
- [Background jobs](../../docs/design/30-background-jobs.md), [email](../../docs/design/31-email.md),
  [RPC wire format](../../docs/design/28-rpc-wire-format.md),
  [endpoint pages](../../docs/design/27-endpoint-pages.md),
  [auth and sessions](../../docs/design/26-auth-and-sessions.md), [money](../../docs/design/34-money.md),
  [bytes and cryptography](../../docs/design/33-crypto.md),
  [HTTP client](../../docs/design/32-http-client.md).
- [Persistence](../../docs/design/19-persistence.md), [testing](../../docs/design/21-testing.md),
  [errors](../../docs/design/09-errors.md), [async data](../../docs/design/14-async-data.md),
  [forms](../../docs/design/15-forms-and-validation.md).
