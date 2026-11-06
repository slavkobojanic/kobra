# RPC Wire Format and Failures

## Decision

Every protocol has **deterministic, locked Protobuf names**, so a non-K client (or `curl` in an end-to-end test) can call an rpc without reading generated files. **Every way an rpc can fail outside its handler arrives as one typed case**, `rpcFailure(RpcFailure)`, in the rpc's own error enum.

## Names

| K | Protobuf / Connect |
|---|---|
| module `store` in project `shop` ([24-projects.md](24-projects.md)) | package `shop.store` |
| `protocol StoreProtocol` | service `StoreProtocol` |
| `rpc placeOrder(request: OrderRequest)` | method `PlaceOrder` (upper camel case) |
| URL | `POST /shop.store.StoreProtocol/PlaceOrder` |

- **The project prefix is the Protobuf form of `k.toml`'s `name`:** lower case, with `-`, `/` and `.` mapped to `_`, and a package's host dropped (`my-shop` → `my_shop`).
- **The names are recorded per protocol in `protocol.lock`** when first derived. From then on the lock is the source: renaming the project, renaming the protocol or an rpc, or moving the protocol's file to another module keeps the wire names, as a named URL does. Changing a recorded name requires `@Breaking` ([11-protocols-and-rpc.md](11-protocols-and-rpc.md#protocollock)).

## Messages

- **Each rpc has a generated request message whose fields are its parameters.** The wire field is the parameter's **internal name**, which is never empty and never a keyword: `func reviews(for product: ProductId)` takes `{"product": …}`, and `get(_ id: Uuid)` takes `{"id": …}`. An rpc without parameters takes `{}`.
  - Changing a parameter's external label is source-breaking but wire-compatible.
  - Changing its internal name renames the wire field, which `protocol.lock` records, so it needs `@Breaking`.
- **A struct return type is the response message itself:** `placeOrder` answers `{"orderId": "…", "paymentUrl": "…"}`. Other return types are wrapped as `{"value": …}`, and an rpc without a return type answers `{}`.
- JSON field names are the K field names (lower camel case, which is also Protobuf's JSON mapping). Enums without payloads are their case names (`"awaitingPayment"`); `Uuid`, `Url` and `EmailAddress` are strings; `Date` is RFC 3339; `Money` is as in [34-money.md](34-money.md).
- **Servers accept both encodings,** binary Protobuf (`application/proto`) and JSON (`application/json`), in every environment, and nothing else ([26-auth-and-sessions.md](26-auth-and-sessions.md#request-forgery)). K clients send binary in production and JSON in development; other clients choose.
- **Request bodies are limited to 1 MB,** enforced before decoding, like endpoint pages.
- Every rpc response carries `Cache-Control: private, no-store`.

## Failures outside the handler

An rpc call can fail before its handler runs. Those failures are one stdlib enum in `k.rpc`:

```k
public enum RpcFailure implements Error {
    case rejected(GuardRejection)       // a guard refused the call
    case invalid(ValidationError)       // a Validatable parameter failed validation
}
```

- **An rpc that can fail before its handler runs** (it has a guard, its own or its protocol's, or a `Validatable` parameter) **must declare an error enum with exactly one case `rpcFailure(RpcFailure)`.** The name is fixed, so the runtime never has to find the case by its shape. (It isn't `rpc`, which is a keyword.)
- A guarded or validated rpc without that case is a compile error, with a quick fix that adds the case to its error enum. **An rpc with no other errors declares `throws RpcOnlyError`,** a stdlib enum whose only case is `rpcFailure(RpcFailure)`.
- **Only the runtime produces `rpcFailure`.** A handler can't throw it. Client code that wraps errors at a layer boundary passes it on like any other case (`catch .rpcFailure(let failure) { throw LookupError.rpcFailure(failure) }`).
- **A `@Form` shows `.invalid` under its fields** when the submit action rethrows the `ValidationError` ([15-forms-and-validation.md](15-forms-and-validation.md)).
- Guards and validation also run on in-process calls during server rendering, so `rejected` and `invalid` can happen there too.

```k
public enum CheckoutError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case paymentUnavailable
    case rpcFailure(RpcFailure)
}

protocol StoreProtocol {
    rpc placeOrder(request: OrderRequest) throws CheckoutError -> PlacedOrder requires RateLimit(10.perMinute)
}
```

### Open question: network failures on the client

**Not decided.** When a browser can't reach the server during an rpc (offline, or the request timed out), the call fails in a way no handler chose. Two designs are on the table:

- **Typed (recommended).** `RpcFailure` gains `case offline` and `case timeout`. Every rpc called over the network from the client then needs the `rpcFailure` case, not only guarded or validated ones, so every rpc declares an error enum (`RpcOnlyError` when it has no other errors). During server rendering and in-process calls, `offline` and `timeout` never occur. A dropped connection while submitting a form or saving a draft becomes a value the page handles, consistent with [09-errors.md](09-errors.md) and with `HttpError.offline` for external APIs ([32-http-client.md](32-http-client.md)). The cost is one error enum on every rpc.
- **Panic.** Network failures stay outside the type system: they panic and reach the nearest `ErrorBoundary`, and the stdlib adds an `OfflineBoundary` that shows a retry state. Rpcs that are neither guarded nor validated keep having no error enum. The cost is that an unsaved form or draft is lost through a panic on a normal web condition.

Until this is decided, the text above specifies only `rejected` and `invalid`, and what a client sees on a network failure is unspecified.

## Errors and status codes

| Outcome | Connect code | HTTP status |
|---|---|---|
| a declared error case (`CheckoutError.outOfStock(available: 1)`) | `failed_precondition`, with the case and its payload in the error details | 400 |
| `rpcFailure(.invalid(…))`: a `Validatable` parameter fails validation ([15-forms-and-validation.md](15-forms-and-validation.md)) | `invalid_argument`, with field paths and messages | 400 |
| `rpcFailure(.rejected(.unauthenticated))` | `unauthenticated` | 401 |
| `rpcFailure(.rejected(.forbidden))` | `permission_denied` | 403 |
| `rpcFailure(.rejected(.rateLimited(retryAfter:)))` | `resource_exhausted`, with a `Retry-After` header | 429 |
| a request body over 1 MB | `resource_exhausted` | 413 |
| a content type other than JSON or Protobuf, or no `Connect-Protocol-Version` | — | 415 |
| a panic | `internal`, with no details outside development | 500 |

- **On a K client, each of these arrives typed** as the rpc's error enum, except a panic.
- Error types that cross the wire are `Codable`. The compiler warns when an error payload is a server-only type or an upstream response (an `HttpError` holding a third party's response body), with a hint to wrap the error without it.

## Calling an rpc by hand

```bash
curl --header 'Content-Type: application/json' --header 'Connect-Protocol-Version: 1' \
     --data '{"request":{"productSlug":"k-mug","quantity":2,"email":"ana@example.com"}}' \
     http://localhost:8080/shop.store.StoreProtocol/PlaceOrder
```

## Why

- Deterministic names derived once and then locked mean `k proto export` is never needed just to find a URL, and refactoring never breaks a non-K client.
- One request message per rpc keeps adding a parameter a compatible change (a new field).
- One fixed failure case replaces a naming convention and hidden panics with an exhaustive `catch`.
- Standard Connect codes let generic infrastructure (load balancers, gRPC clients, monitoring) understand K errors without K-specific knowledge.
