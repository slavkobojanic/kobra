# Services and Dependency Injection

## Decision

`service` is a declaration keyword. **A service is both the unit of dependency injection and the container for shared state.** There is no separate store concept (Pinia, Redux, Zustand).

```k
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
}
```

## Injection

- **Injection is resolved at compile time.** The compiler builds the dependency graph. A cycle or a missing implementation is a compile error. There is no runtime reflection.
- **Fields are injected with `@Inject`.** Spring prefers constructor injection for testability, but K tests override bindings with `use` rules ([21-testing.md](21-testing.md)), so the more readable form is used.
- **Injected fields are always private** to the declaration that injects them ([16-modules.md](16-modules.md)).

## Initialization: `init()`

A service may declare **one parameterless `init()`**. It runs once per instance, after every `@Inject` field is resolved and every field initializer has run, and before the instance is given to anything that injects it.

```k
public service TodoService {
    @Inject let storage: LocalStorage

    @State var items: List<Todo> = []
    @State var problem: StorageProblem? = nil

    /// Restores the saved todos as soon as the service is created.
    init() {
        restore()
    }

    func restore() { ... }
}
```

- **No parameters.** A service's inputs are its injected dependencies. `init(...)` with parameters is a compile error.
- **Synchronous and non-throwing.** Nothing could await `init()` or catch its error, because the container creates the instance. Errors are handled inside and recorded in state, as `TodoService` does with `problem`. Async data belongs in `@Load` ([14-async-data.md](14-async-data.md)) or in a method the caller awaits.
- **It may write the service's own `@State`** and call injected services. The dependency graph is acyclic, so a dependency's `init()` has always finished first.
- **It runs when the instance is created,** which follows the scope (below). In tests, every test gets a fresh container, so `init()` runs again for each test.
- **In a client-only service** (below), `init()` never runs during server rendering. In the browser it runs after hydration, so a server-rendered page shows the service's declared initial state first.
- **Only services.** Components and pages have field initializers for setup and `@Effect` for side effects.
- The formatter places `init()` after the fields and before the other functions.

## Scopes

| Scope | On the server | In the browser |
|---|---|---|
| `@Scope(.app)` | **Default.** One instance per request, or per background job run | One instance per app |
| `@Scope(.request)` | One instance per request. The service is server-only. | — |
| `@Scope(.route)` | One instance per request | Lives as long as the route is mounted |
| `@Scope(.component)` | Each injecting subtree gets its own instance, for example one `FormService` per form | the same |
| `@Scope(.server)` | One instance per server process, shared by every request. The service is server-only. | — |

- **Server rendering always isolates service state per request,** so state can't leak between users. `.app` on the server is therefore per request, which makes `.server` the only process-wide scope.
- **Each background job run gets a fresh container** that works like a request's ([30-background-jobs.md](30-background-jobs.md)).

### Scope compatibility

A service may inject only services that live at least as long as it does, so it can never hold on to an instance from a request that has ended:

| A service in scope | May inject services in scope |
|---|---|
| `.server` | `.server` only |
| `.app`, `.request` | `.server`, `.app`, `.request` |
| `.route` | `.server`, `.app`, `.request`, `.route` |
| `.component` | any |

- **Stateless injectables work in every scope:** repositories, `Database`, configs ([25-config-and-secrets.md](25-config-and-secrets.md)) and stdlib platform interfaces.
- **A `.server` service can't inject `Session`** or anything request-scoped, because it would capture one request's session for the whole process. The compile error names the injection path.
- **A job's dependency graph can't reach `Session` or `SessionManager`.** A job runs outside any request, so it has no session. The compile error names the path; a job that needs a user's id carries it in its payload.

## State ownership

**Only a service's own methods can write its fields,** `@State` or plain `var`. Components can read `cart.items` but can't assign `cart.items = []`, and a test can read a fake's recorded calls but not overwrite them. All state changes go through methods, which keeps them auditable.

## Layering

The compiler enforces fixed layering rules. **Components never call protocols directly. Remote calls go through services.**

| Kind | May inject |
|---|---|
| `component`, `page`, `layout` | services |
| endpoint page ([27-endpoint-pages.md](27-endpoint-pages.md)) | services, including server-only ones |
| `service` | services, protocols and stdlib platform interfaces; if the service is server-side, also repositories, configs, `Database` and `SessionManager` |
| `service … implements Protocol` | the same as a service; always server-only |
| `repository` | other repositories and configs; server-only |
| `config` | nothing; a config is data |
| functions, `struct`, `enum` | nothing; importable anywhere |

**Stdlib platform interfaces** wrap I/O the platform provides. They are injectable only into services, like protocols, so retries, caching and error wrapping stay in one place. Each is declared in the stdlib as client-only, server-only or usable on both sides:

| Interface | Side | Default implementation |
|---|---|---|
| `HttpClient` ([32-http-client.md](32-http-client.md)) | both | `FetchHttpClient` |
| `LocalStorage` ([36-local-storage.md](36-local-storage.md)) | client-only | `BrowserStorage` |
| `EmailProvider` ([31-email.md](31-email.md)) | server-only | `SmtpEmailProvider` |
| `JobQueue` ([30-background-jobs.md](30-background-jobs.md)) | server-only | the database queue |

**`Clock` is a stdlib service, not a platform interface** ([35-time.md](35-time.md)). Components, pages, layouts and services may inject it like any service, and it holds no request state, so services in every scope may inject it. Plain functions and structs take the current time as a `now: Date` parameter.

```k
component AddToCart {
    @Inject let cart: CartService       // allowed
    @Inject let api: CartProtocol       // compile error: components can't inject protocols
}
```

Why:

- Remote-call logic (optimistic updates, caching, retries, syncing results into state) lives in one place.
- Component tests only need to mock services.
- The rules are fixed, not configurable, so every K codebase has the same architecture.

## Server/client boundary

**The boundary is drawn per declaration.** Every declaration is server-only, client-only, or usable on both sides; there is no per-member boundary.

- **Server-only:** entities, repositories, configs, endpoint pages, the generated `Endpoint` enum, and every service that implements a protocol, is in `.request` or `.server` scope, implements `Job`, or injects anything server-only (a repository, a config, `Database`, `SessionManager`, a server-only platform interface or service).
- **Client-only:** every service that injects a client-only platform interface or service (`LocalStorage`, `Theme`).
- **A service that would be both** is a compile error naming both paths.
- **Importing a server-only declaration into client code is a compile error,** which names the path by which it would reach the browser. Secrets can't end up in the client bundle.
- **Client-only services during server rendering:** in an app that renders on the server ([23-app-structure.md](23-app-structure.md)), a client-only service still exists, with its declared initial state, so pages can render. Its `init()` doesn't run. Calling a client-only interface from code that can run during server rendering (a field initializer, a `@Derived` or `@Load` initializer, a view) is a compile error; `init()`, effects and event handlers are fine, because they run only in the browser. A server-rendered page that reads a client-only service's state is a warning, with a quick fix that adds `@Rendering(.client)` to the app, because the first paint would show the initial state. When the read is in a component, the first fix renders only that component in the browser ([47-client-rendered-components.md](47-client-rendered-components.md)); neither rule applies inside a client-rendered component's subtree.
- When a type needs a server-only and a client-safe face, it is two declarations. `Session` (who is signed in, readable everywhere) and `SessionManager` (signing in and out, server-only) are the stdlib's example ([26-auth-and-sessions.md](26-auth-and-sessions.md)).

## Injecting interfaces

```k
@Inject let payments: PaymentProvider
```

- **An interface with exactly one implementation is injected automatically.** Stdlib interfaces count their stdlib default; tests get a separate default ([21-testing.md](21-testing.md#test-doubles-and-defaults)).
- **With several implementations, the choice is made explicitly** in `App.k`, optionally per environment ([23-app-structure.md](23-app-structure.md)):

  ```k
  use StripePayments for PaymentProvider in .production
  use FakePayments for PaymentProvider in .development, .test
  ```

- An ambiguous or missing implementation is a compile error.
- The keyword is `use`, not `bind`, because `bind` already means two-way binding.

## Open questions

- Whether `@Scope(.route)` and `@Scope(.component)` services need a teardown counterpart to `init()`, for example to cancel timers. `onCleanup` inside `init()` would match effects.
