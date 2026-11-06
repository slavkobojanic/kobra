# Services and dependency injection

`service` is a declaration keyword. **A service is both the unit of dependency injection and the
container for shared state.** There is no separate store concept: no Pinia, no Redux, no Zustand.

```k
service CartService {
    @Inject let api: CartProtocol
    @Inject let auth: AuthService

    @State var items: List<CartItem> = []
    @Derived let total: Money = items.sum { item => item.price * item.quantity }

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

- **Fields are injected with `@Inject`.** Injected fields are always `let`.
- **Injection is resolved at compile time.** The compiler builds the dependency graph and generates the
  wiring code. A cycle or a missing implementation is a compile error. There is no runtime reflection, and
  the output is tree-shakeable.
- There is no constructor injection. Tests replace dependencies with `use … for …` instead (see
  [Testing](testing.md)), so the more readable field form is the only form.

## State ownership

**Only a service's own methods can write its `@State`.** Components can read `cart.items`, but they can't
assign `cart.items = []`. Every change goes through a method, which keeps state changes auditable: to find
everything that can change the cart, read `CartService`.

## Setup with `init()`

> **Proposal.** Service initialization follows `docs/design/proposals/service-init.md`.

A service may declare one parameterless `init()`. It runs once per instance, after every injected field is
resolved, which makes it the place to restore saved state:

```k
public service TodoService {
    @Inject let storage: LocalStorage

    @State public var items: List<Todo> = []
    @State public var problem: StorageProblem? = nil

    init() {
        restore()
    }

    public func restore() { … }
}
```

`init()` can't be `async` and can't throw: errors are handled inside and recorded in state.

## Scopes

A service's scope decides how long an instance lives and who shares it:

| Scope | Lifetime |
|---|---|
| `@Scope(.app)` | **The default.** One instance per app in the browser. |
| `@Scope(.request)` | One instance per server-rendered request. |
| `@Scope(.route)` | Lives as long as the route is mounted. |
| `@Scope(.component)` | Each injecting subtree gets its own instance, for example one `FormService` per form. |

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
another user's page.

## Layering

The compiler enforces fixed layering rules. **Components never call protocols directly. Remote calls go
through services.**

| Kind | May inject |
|---|---|
| `component`, `page`, `layout` | services |
| `service` | services, protocols and, if the service is server-side, repositories and other server-only resources (configuration and secrets) |
| `service … implements Protocol` | the same as a service; always server-only |
| `repository` | other repositories; server-only |
| functions, `struct`, `enum` | nothing; importable anywhere |

```k
component AddToCart {
    @Inject let cart: CartService       // allowed
    @Inject let api: CartProtocol       // compile error: components can't inject protocols
}
```

Why fixed rules?

- Remote-call logic (optimistic updates, caching, retries, syncing results into state) lives in one
  place.
- Component tests only need to fake services.
- The rules aren't configurable, so every K codebase has the same architecture.

The same rule covers other platform access: the standard library's `HttpClient` and `LocalStorage` are
injectable only into services.

## The server/client boundary

The compiler splits every app into a server bundle and a client bundle, and enforces the boundary
between them. **Importing a server-only service, or anything it injects (a repository, a configuration,
a secret), into code that runs in the browser is a compile error.** Secrets can't end up in the client
bundle.

A service becomes server-only when it implements a protocol, or injects something server-only. Ordinary
services run in both places: in the browser, and during server rendering.

## Injecting interfaces

You can inject an [interface](../language/declarations.md#interfaces) instead of a concrete service:

```k
@Inject let payments: PaymentProvider
```

- **An interface with exactly one implementation is injected automatically.**
- **With several implementations, the choice is made explicitly** in `App.k`, optionally per
  environment:

  ```k
  use StripePayments for PaymentProvider in .production
  use FakePayments for PaymentProvider in .development, .test
  ```

- An ambiguous or missing implementation is a compile error.

The keyword is `use`, not `bind`, because `bind` already means two-way binding. See
[The app declaration](app.md).
