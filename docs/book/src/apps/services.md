# Services and dependency injection

`service` is a declaration keyword. **A service is both the unit of dependency injection and the
container for shared state.** There is no separate store concept: no Pinia, no Redux, no Zustand.

<!-- The declarations the samples in this chapter use; checked, not rendered.

```k
// src/cart/Context.k
import k.rpc.RpcFailure

struct Product implements Codable {
    let id: Int
    let price: Money
}

struct CartItem implements Codable {
    let price: Money
    let quantity: Int
}

struct AddItemRequest implements Codable {
    let productId: Int
}

enum CartError implements Error {
    case soldOut
    case rpcFailure(RpcFailure)
}

protocol CartProtocol {
    rpc add(request: AddItemRequest) throws CartError -> CartItem
}

service CartApi implements CartProtocol {
    rpc add(request: AddItemRequest) async throws CartError -> CartItem {
        throw .soldOut
    }
}

struct Receipt {
    let total: Money
}

```

```k
// src/payments/PaymentProvider.k
/// Takes payments.
public interface PaymentProvider {
    /// Charges `amount`.
    func charge(amount: Money) async -> Money
}
```

```k
// src/payments/StripePayments.k
/// Payments through Stripe.
public service StripePayments implements PaymentProvider {
    func charge(amount: Money) async -> Money {
        return amount
    }
}
```

```k
// src/payments/FakePayments.k
/// Payments that always succeed.
public service FakePayments implements PaymentProvider {
    func charge(amount: Money) async -> Money {
        return amount
    }
}
```
-->

```k
~import k.auth.Session
service CartService {
    @Inject let api: CartProtocol
    @Inject let session: Session

    @State var items: List<CartItem> = []
    @Derived let total: Money = items.sum(in: .usd) { item => item.price * item.quantity }

    func add(product: Product) async throws CartError {
        let item = try await api.add(request: AddItemRequest(productId: product.id))
        items.append(item)
    }
}

component CartBadge {
    @Inject let cart: CartService

    view {
        text { "{cart.items.count}" }
            .class(.badge)
    }

    style {
        .badge { fontWeight: bold; }
    }
}
```

Every component that injects `CartService` gets the same instance, so the badge, the cart page and the
checkout button all see the same items.

## Injection

- **Fields are injected with `@Inject`.** Injected fields are always `let`, and always private to the
  declaration that injects them: a type's dependencies are never part of its API.
- **Injection is resolved at compile time.** The compiler builds the dependency graph and generates the
  wiring code. A cycle or a missing implementation is a compile error. There is no runtime reflection, and
  the output is tree-shakeable.
- There is no constructor injection. Tests replace dependencies with `use … for …` instead (see
  [Testing](testing.md)), so the more readable field form is the only form.

## State ownership

**Only a service's own methods can write its fields,** `@State` or plain `var`. Components can read
`cart.items`, but they can't assign `cart.items = []`. Every change goes through a method, which keeps
state changes auditable: to find everything that can change the cart, read `CartService`.

## Setup with `init()`

A service may declare **one parameterless `init()`**. It runs once per instance, after every `@Inject`
field is resolved and every field initializer has run, and before the instance is handed to anything
that injects it. It is the place to restore saved state:

```k
// examples/todos/src/todo/TodoService.k
public service TodoService {
    @Inject let storage: LocalStorage
    // ...
    @State var items: List<Todo> = []
    // ...
    @State var problem: StorageProblem? = nil
    // ...
    init() {
        restore()
    }
    // ...
}
```

- **No parameters.** A service's inputs are its injected dependencies.
- **Synchronous and non-throwing.** Nothing could await `init()` or catch its error, because the
  container creates the instance. Errors are handled inside and recorded in state, as `TodoService` does
  with `problem`. Async data belongs in [`@Load`](async-data.md) or in a method the caller awaits.
- **It may write the service's own `@State`** and call injected services. The dependency graph is
  acyclic, so a dependency's `init()` has always finished first.
- In tests, every test gets a fresh container, so `init()` runs again for each test.
- In a client-only service, `init()` never runs during server rendering. In the browser it runs after
  hydration.
- Only services have `init()`. Components and pages use field initializers for setup and `@Effect` for
  side effects. The formatter places `init()` after the fields and before the other functions.

## Scopes

A service's scope decides how long an instance lives and who shares it:

| Scope | On the server | In the browser |
|---|---|---|
| `@Scope(.app)` | **The default.** One instance per request, or per background job run | One instance per app |
| `@Scope(.request)` | One instance per request. The service is server-only. | — |
| `@Scope(.route)` | One instance per request | Lives as long as the route is mounted |
| `@Scope(.component)` | Each injecting subtree gets its own instance, for example one `FormService` per form | the same |
| `@Scope(.server)` | One instance per server process, shared by every request. The service is server-only. | — |

```k
@Scope(.component)
service WizardState {
    @State var step: Int = 1

    func next() {
        step += 1
    }
}
```

**Server rendering always isolates service state per request**, so one user's state can never leak into
another user's page. That is why `.app` on the server means one instance per request, and why `.server`
is the only process-wide scope. It exists for deliberate sharing between requests, such as the
`Broadcast` behind a [server stream](streaming.md#sharing-events-between-requests).

### Scope compatibility

A service may inject only services that live at least as long as it does, so it can never hold on to an
instance from a request that has ended:

| A service in scope | May inject services in scope |
|---|---|
| `.server` | `.server` only |
| `.app`, `.request` | `.server`, `.app`, `.request` |
| `.route` | `.server`, `.app`, `.request`, `.route` |
| `.component` | any |

- **Stateless injectables work in every scope:** repositories, `Database`, configs and standard-library
  platform interfaces.
- **A `.server` service can't inject `Session`** or anything request-scoped, because it would capture
  one request's session for the whole process. The compile error names the injection path.

## Layering

The compiler enforces fixed layering rules. **Components never call protocols directly. Remote calls go
through services.**

| Kind | May inject |
|---|---|
| `component`, `page`, `layout` | services |
| endpoint page | services, including server-only ones |
| `service` | services, protocols and platform interfaces; if the service is server-side, also repositories, configs, `Database` and `SessionManager` |
| `service … implements Protocol` | the same as a service; always server-only |
| `repository` | other repositories and configs; server-only |
| `config` | nothing; a config is data |
| functions, `struct`, `enum` | nothing; importable anywhere |

```k,fails=K4005
~service CartService {
~}
~
component AddToCart {
    @Inject let cart: CartService       // allowed
    @Inject let api: CartProtocol       // compile error: components can't inject protocols
~
~    view {
~        text { "Add" }
~    }
}
```

Why fixed rules?

- Remote-call logic (optimistic updates, caching, retries, syncing results into state) lives in one
  place.
- Component tests only need to fake services.
- The rules aren't configurable, so every K codebase has the same architecture.

### Platform interfaces

I/O the platform provides is reached through standard-library interfaces. Like protocols, they are
injectable only into services, so retries, caching and error wrapping stay in one place. Each has one
production implementation, so it is injected with no `use` rule, and a hermetic default in tests:

| Interface | Side | Production implementation | Chapter |
|---|---|---|---|
| `HttpClient` | both | `FetchHttpClient` | [Calling HTTP APIs](http.md) |
| `LocalStorage` | client-only | `BrowserStorage` | [Local storage](local-storage.md) |
| `EmailProvider` | server-only | `SmtpEmailProvider` | [Email](email.md) |
| `JobQueue` | server-only | the database queue | [Background jobs](jobs.md) |

**`Clock` is a standard-library service, not a platform interface** ([Time and timers](time.md)).
Components, pages, layouts and services may inject it like any service, and it holds no request state,
so services in every scope may inject it. Plain functions and structs take the current time as a
`now: Date` parameter.

## The server/client boundary

The compiler splits every app into a server bundle and a client bundle, and **draws the boundary per
declaration**: every declaration is server-only, client-only, or usable on both sides.

- **Server-only:** entities, repositories, configs, endpoint pages, and every service that implements a
  protocol, is in `.request` or `.server` scope, implements `Job`, or injects anything server-only (a
  repository, a config, `Database`, `SessionManager`, a server-only platform interface or service).
- **Client-only:** every service that injects a client-only platform interface or service, such as
  `LocalStorage` or `Theme`.
- **A service that would be both** is a compile error naming both paths.
- **Importing a server-only declaration into client code is a compile error,** which names the path by
  which it would reach the browser. Secrets can't end up in the client bundle.

Ordinary services run in both places: in the browser, and during server rendering. When a type needs a
server-only face and a client-safe one, it is two declarations: `Session` (who is signed in, readable
everywhere) and `SessionManager` (signing in and out, server-only) are the standard library's example
([Authentication and sessions](auth.md)).

### Client-only services during server rendering

In an app that renders on the server, a client-only service still exists during server rendering, with
its declared initial state, so pages can render. Its `init()` doesn't run.

- Calling a client-only interface from code that can run during server rendering (a field initializer,
  a `@Derived` or `@Load` initializer, a view) is a compile error. `init()`, effects and event handlers
  are fine, because they run only in the browser.
- A server-rendered page that reads a client-only service's state is a warning, with a quick fix that
  adds `@Rendering(.client)` to the app, because the first paint would show the initial state and then
  jump. See [The app declaration](app.md#rendering-on-the-client).

## Injecting interfaces

You can inject an [interface](../language/declarations.md#interfaces) instead of a concrete service:

```k
~import payments.PaymentProvider
~
~service Checkout {
@Inject let payments: PaymentProvider
~}
```

- **An interface with exactly one implementation is injected automatically.** Standard-library
  interfaces count their built-in default.
- **With several implementations, the choice is made explicitly** in `App.k`, optionally per
  environment:

  ```k
  // App.k
  ~import payments.FakePayments
  ~import payments.PaymentProvider
  ~import payments.StripePayments
  ~
  ~app Shop {
  use StripePayments for PaymentProvider in .production
  use FakePayments for PaymentProvider in .development, .test
  ~}
  ```

- An ambiguous or missing implementation is a compile error.

The keyword is `use`, not `bind`, because `bind` already means two-way binding. See
[The app declaration](app.md).
