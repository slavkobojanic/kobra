# Services and Dependency Injection

## Decision

`service` is a declaration keyword. **A service is both the unit of dependency injection and the container for shared state.** There is no separate store concept (Pinia, Redux, Zustand).

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
}
```

## Injection

- **Injection is resolved at compile time.** The compiler builds the dependency graph. A cycle or a missing implementation is a compile error. There is no runtime reflection.
- **Fields are injected with `@Inject`.** Spring prefers constructor injection for testability, but K tests override bindings through `TestContainer`, so the more readable form is used.

## Scopes

| Scope | Lifetime |
|---|---|
| `@Scope(.app)` | **Default.** One instance per app in the browser. |
| `@Scope(.request)` | One instance per server-rendered request. |
| `@Scope(.route)` | Lives as long as the route is mounted. |
| `@Scope(.component)` | Each injecting subtree gets its own instance, for example one `FormService` per form. |

**Server rendering always isolates service state per request,** so state can't leak between users.

## State ownership

**Only a service's own methods can write its `@State`.** Components can read `cart.items` but can't assign `cart.items = []`. All state changes go through methods, which keeps them auditable.

## Layering

The compiler enforces fixed layering rules. **Components never call protocols directly. Remote calls go through services.**

| Kind | May inject |
|---|---|
| `component`, `page`, `layout` | services |
| `service` | services, protocols, and, if the service is server-side, repositories and other server-only resources (secrets) |
| `service … implements Protocol` | the same as a service; always server-only |
| `repository` | other repositories; server-only |
| functions, `struct`, `enum` | nothing; importable anywhere |

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

Importing a server-only service, or anything it injects (a database, secrets), into client code is a compile error. Secrets can't end up in the client bundle.

## Injecting interfaces

```k
@Inject let payments: PaymentProvider
```

- **An interface with exactly one implementation is injected automatically.**
- **With several implementations, the choice is made explicitly** in `App.k`, optionally per environment:

  ```k
  use StripePayments for PaymentProvider in .production
  use FakePayments for PaymentProvider in .development, .test
  ```

- An ambiguous or missing implementation is a compile error.
- The keyword is `use`, not `bind`, because `bind` already means two-way binding.
