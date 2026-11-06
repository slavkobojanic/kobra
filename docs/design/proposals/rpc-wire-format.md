# Proposal: RPC Wire Format and HTTP Status Codes

> **Status: proposal.** Not yet accepted into the design. First used by `examples/shop`.

## Problem

[11-protocols-and-rpc.md](../11-protocols-and-rpc.md) says protocols compile to Protobuf and run over Connect, and that typed errors map to Connect error details. It doesn't say what the generated Protobuf package, service and message names are, so a non-K client (or `curl` in an end-to-end test) can't call an rpc without reading generated files. Nor does it say which HTTP status a typed error, a validation failure or a guard rejection produces.

## Proposal

### Names

| K | Protobuf / Connect |
|---|---|
| module `store` in package `shop` (`k.toml` `name`) | package `shop.store` |
| `protocol StoreProtocol` | service `StoreProtocol` |
| `rpc placeOrder(request: OrderRequest)` | method `PlaceOrder` (upper camel case) |
| URL | `POST /shop.store.StoreProtocol/PlaceOrder` |

- These names are part of the contract recorded in `protocol.lock`, so renaming a protocol or rpc is a breaking change that needs `@Breaking` ([11-protocols-and-rpc.md](../11-protocols-and-rpc.md)).

### Messages

- **Each rpc has a generated request message whose fields are its parameters**, named as the parameters are: `placeOrder(request: OrderRequest)` takes `{"request": {…}}`, and `product(slug: String)` takes `{"slug": "k-mug"}`. An rpc without parameters takes `{}`.
- **A struct return type is the response message itself**: `placeOrder` answers `{"orderId": "…", "paymentUrl": "…"}`. Other return types are wrapped as `{"value": …}`, and an rpc without a return type answers `{}`.
- JSON field names are the K field names (lower camel case, which is also Protobuf's JSON mapping). Enums without payloads are their case names (`"awaitingPayment"`); `Uuid` and `Url` are strings; `Date` is RFC 3339; `Money` is as in [money.md](money.md).
- **Servers always accept both encodings,** binary Protobuf (`application/proto`) and JSON (`application/json`), in every environment. K clients send binary in production and JSON in development, as [11-protocols-and-rpc.md](../11-protocols-and-rpc.md) describes; other clients choose.

### Errors and status codes

| Outcome | Connect code | HTTP status |
|---|---|---|
| a declared error case (`CheckoutError.outOfStock(available: 1)`) | `failed_precondition`, with the case and its payload in the error details | 400 |
| a `Validatable` parameter fails validation ([15-forms-and-validation.md](../15-forms-and-validation.md)) | `invalid_argument`, with field paths and messages | 400 |
| `GuardRejection.unauthenticated` | `unauthenticated` | 401 |
| `GuardRejection.forbidden` | `permission_denied` | 403 |
| `GuardRejection.rateLimited(retryAfter:)` | `resource_exhausted`, with a `Retry-After` header | 429 |
| a panic | `internal`, with no details outside development | 500 |

- **On a K client, each of these arrives typed.** Declared cases and guard rejections arrive as the rpc's error enum ([auth-and-sessions.md](auth-and-sessions.md)). A validation failure arrives at the `@Form` that submitted it and is shown under its fields, as [15-forms-and-validation.md](../15-forms-and-validation.md) describes; outside a form it panics, because client code should have validated first.
- Network failures on the client are left to a separate proposal; today they panic and reach the nearest `ErrorBoundary`.

### Calling an rpc by hand

```bash
curl --header 'Content-Type: application/json' --header 'Connect-Protocol-Version: 1' \
     --data '{"request":{"productSlug":"k-mug","quantity":2,"email":"ana@example.com"}}' \
     http://localhost:8080/shop.store.StoreProtocol/PlaceOrder
```

## Why

- Deterministic names derived from the module path and declaration names mean `k proto export` is never needed just to find a URL, and the names follow Protobuf's own conventions.
- One request message per rpc keeps adding a parameter a compatible change (a new field), which matters for the `protocol.lock` compatibility rules.
- Standard Connect codes let generic infrastructure (load balancers, gRPC clients, monitoring) understand K errors without K-specific knowledge.
