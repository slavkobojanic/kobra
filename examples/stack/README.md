# Stack

Two services and the contract library between them, in one workspace
([`docs/design/40-workspaces-and-services.md`](../../docs/design/40-workspaces-and-services.md)):

- **paybox** takes payments. It serves `pay.Payments` to the other services and has no pages.
- **shop** is a storefront. Its page pays through its own server, which calls paybox; paybox then tells
  the shop about the charge through `pay.PaymentEvents`, which the shop serves.
- **pay-contracts** is a library holding both protocols and the types they carry. The services call each
  other, so they share this library instead of depending on each other: apps can't be dependencies, and
  the dependency graph must stay acyclic.

Nothing here needs an account anywhere: the services only call each other.

## Running it

From this directory:

| Command | What it does |
|---|---|
| `k dev` | Builds and serves both apps: paybox on 8080 and shop on 8081. Each `remote` rule is wired to the other app with a development secret generated for the run, so no `.env` is needed. Open http://localhost:8081 and press **Pay**. |
| `k test` | Runs both apps' tests. Remote calls never leave the process: paybox's `PaymentEvents` is a `TestRemote` double, and shop's tests bind a fake `Payments`. |
| `k check` | Checks every member, and the `rpc.lock` of each app against the code, and runs both apps' tests. |
| `k build` | Builds `dist/paybox/` and `dist/shop/`, and updates each app's `rpc.lock`. |

To run one app on its own, copy its `.env.example` to `.env` and run `k dev` in its directory.

## Structure

```
stack/
├── k.toml                      [workspace] members
├── pay-contracts/              library (no App.k)
│   └── src/pay/                Payments, PaymentEvents, ChargeRequest, Charge, ChargeError
├── paybox/                     app: serves Payments, calls PaymentEvents
│   ├── App.k                   use remote(ShopService) for PaymentEvents
│   ├── rpc.lock                generated: the shape of Payments, which deployed callers rely on
│   └── src/payments/           PaymentsApi, ChargeLog, ShopService (a RemoteService config)
└── shop/                       app: serves CheckoutApi and PaymentEvents, calls Payments
    ├── App.k                   use remote(PayboxService) for Payments
    ├── rpc.lock                generated
    └── src/
        ├── checkout/           Checkout (calls paybox; server-only), CheckoutApi for the page, Cart
        ├── payments/           PayboxService (a RemoteService config), PaymentEventsApi, PaidOrders
        └── home/               the page
```

## In production

Each app is a `dist/<name>/app` binary. Give both the same secret: paybox reads it as `K_SERVICE_TOKEN`
and `SHOP_TOKEN`, shop as `K_SERVICE_TOKEN` and `PAYBOX_TOKEN`; and tell each where the other runs
(`SHOP_URL`, `PAYBOX_URL`). Every call carries a token signed with that secret, valid for one minute.
