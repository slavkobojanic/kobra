# Stack: the smallest system of services

`examples/stack` is the smallest workspace that calls between services: two apps and the contract
library between them. The [shop example](shop.md) is the same shape grown into a production-style
system; this one fits on a page, so it is the place to see the mechanism.

- **paybox** takes payments. It serves `pay.Payments` and has no pages.
- **shop** is a storefront. Its page pays through its own server, which calls paybox; paybox then tells
  the shop about the charge through `pay.PaymentEvents`, which the shop serves.
- **pay-contracts** is a library holding both protocols and the types they carry. The services call
  each other, so they share this library instead of depending on each other: an app can't be a
  dependency, and the dependency graph must stay acyclic.

```text
stack/
├── k.toml                      [workspace] members
├── pay-contracts/              library (no App.k): Payments, PaymentEvents, ChargeRequest, Charge, ChargeError
├── paybox/                     app: serves Payments, calls PaymentEvents
└── shop/                       app: serves CheckoutApi and PaymentEvents, calls Payments
```

## Running it

| Command, at `examples/stack` | What it does |
|---|---|
| `k dev` | Builds and serves both apps: paybox on 8080 and shop on 8081, each `remote` rule wired to the other with a development secret generated for the run. Open `http://localhost:8081` and press **Pay**. |
| `k test` | Runs both apps' tests. Remote calls never leave the process. |
| `k check` | Checks every member, and each app's `rpc.lock` against its code. |
| `k build` | Builds `dist/paybox/` and `dist/shop/`, and updates each `rpc.lock`. |

## The workspace

```toml
# examples/stack/k.toml
[workspace]
members = ["pay-contracts", "paybox", "shop"]
```

```toml
# examples/stack/shop/k.toml
[package]
name = "shop"
title = "Stack shop"
description = "A storefront that pays through paybox."
k = "0.1"

[dependencies]
pay-contracts = { workspace = true }
```

## Both directions

The library declares both protocols; each app serves one and calls the other:

```k
// examples/stack/pay-contracts/src/pay/PaymentEvents.k
public protocol PaymentEvents requires ServiceAuthenticated {
    // ...
    rpc paid(charge: Charge) throws RpcOnlyError
}
```

```k
// examples/stack/paybox/App.k
public app Paybox {
    use remote(ShopService) for PaymentEvents
}
```

```k
// examples/stack/shop/App.k
public app Shop {
    use remote(PayboxService) for Payments
}
```

The shop serves `PaymentEvents` like any protocol:

```k
// examples/stack/shop/src/payments/PaymentEventsApi.k
public service PaymentEventsApi implements PaymentEvents {
    @Inject let orders: PaidOrders

    rpc paid(charge: Charge) async throws RpcOnlyError {
        orders.record(charge: charge)
    }
}
```

and pays through paybox from a server-side service, because a remote client holds a service token
and is server-only:

```k
// examples/stack/shop/src/checkout/Checkout.k
public service Checkout {
    @Inject let payments: Payments

    /// Charges `cents` for `orderId`; the charge's id.
    func pay(orderId: String, cents: Int) async throws CheckoutError -> Receipt {
        do {
            let charge = try await payments.charge(request: ChargeRequest(orderId: orderId, cents: cents))
            return Receipt(chargeId: charge.id, cents: charge.cents)
        } catch ChargeError.declined {
            throw CheckoutError.declined
        } catch ChargeError.rpcFailure(_) {
            throw CheckoutError.paymentsUnavailable
        }
    }
}
```

The page reaches `Checkout` through the shop's own `CheckoutApi`, so the browser never needs a token
and calls between services never need CORS. [Workspaces and services](../apps/workspaces.md) explains
every rule this example follows, and its samples come from these files.

## In production

Each app is a `dist/<name>/app` binary. Give both the same secret: paybox reads it as `K_SERVICE_TOKEN`
and `SHOP_TOKEN`, shop as `K_SERVICE_TOKEN` and `PAYBOX_TOKEN`; and tell each where the other runs
(`SHOP_URL`, `PAYBOX_URL`). Every call carries a token signed with that secret, valid for one minute.
