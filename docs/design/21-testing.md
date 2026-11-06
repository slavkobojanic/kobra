# Testing

## Decision

```k
// CartService.test.k: lives next to CartService.k, in the same module
test CartService {
    use FakeCartApi for CartProtocol

    @Inject let cart: CartService

    test "adding a product appends it to the cart" {
        try await cart.add(product: .sample)

        expect cart.items.count == 1
        expect cart.total == 9.99.usd
    }

    test "out of stock surfaces a typed error" {
        expect throws CartError.outOfStock {
            try await cart.add(product: .soldOut)
        }
    }
}

test AddToCart {
    use FakeCartService for CartService

    test "clicking add shows the new total" {
        let screen = render(AddToCart(product: .sample))

        screen.button(named: "Add to cart").click()

        expect screen.text("Total: $9.99").isVisible
    }
}
```

## Rules

- **Test files sit next to the source as `Name.test.k`,** in the same module, so they can test package-private code.
- **`test` is a keyword.** `test Name { }` groups tests and holds DI overrides (`use X for Y`, the same syntax as `App.k`). `test "description" { }` declares a single test.
- **`expect` is a keyword that takes any boolean expression.** There is no matcher library (`toEqual`, `toHaveLength`). On failure, the compiler shows the value of each sub-expression, for example: `cart.items.count` was `0`, expected `== 1`.
- **`expect throws E { ... }`** checks that a block throws a specific error case.
- **Components are tested through accessibility queries:** `button(named:)`, `textField(labeled:)`, `text(...)`. An element a test can't find by role and name isn't accessible either.
- **Integration tests run the real stack in-process.** Protocols call their server implementations directly. Each test runs inside a SQLite transaction that is rolled back afterwards.
- **There are no snapshot tests.** They encourage accepting changes without reviewing them.
- `k test` runs tests, with a watch mode. `k check` runs them in CI.
