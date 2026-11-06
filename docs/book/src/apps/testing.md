# Testing

Testing is part of the language. `test` and `expect` are keywords, tests sit next to the code they test,
and the same dependency-injection rules that build your app let tests replace any part of it.

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
```

## Test files and groups

- **Test files sit next to the source as `Name.test.k`**, in the same module, so they can test
  module-private code.
- **`test Name { }` groups tests.** A group holds dependency overrides (`use X for Y`, the same syntax as
  in `App.k`), injected fields and helper functions.
- **`test "description" { }` declares a single test.**
- `k test` runs them; `k test --watch` re-runs affected tests on save; `k check` runs them in CI.

## `expect`

**`expect` takes any boolean expression.** There is no matcher library: no `toEqual`, no
`toHaveLength`. When an expectation fails, the test runner shows the value of each sub-expression:

```text
expect cart.items.count == 1
       cart.items.count was 0, expected == 1
```

**`expect throws E { … }`** checks that a block throws a specific error case. It takes a pattern, as
`catch` does, so nested cases can be checked: `expect throws WeatherError.network(.timeout) { … }`.

## Replacing dependencies

`use X for Y` replaces a dependency for every test in the group:

```k
test CheckoutService {
    use FakePayments for PaymentProvider
    use TestClock for Clock

    @Inject let checkout: CheckoutService
    …
}
```

> **Proposal.** Fakes of concrete services, reaching fakes from tests, fixtures and test-only modules
> follow `docs/design/proposals/testing-support.md`.

- **Every test gets a fresh container**, so no state leaks between tests.
- After `use FakeX for X`, a group can `@Inject let x: FakeX` to configure the fake and inspect what it
  recorded.
- In test-only code, `service FakeCartService implements CartService` is allowed: a fake conforms to a
  concrete service's public API. Outside tests, only interfaces can be implemented.
- Shared fakes and fixtures go in a `testing` directory, a test-only module that never reaches a build.
  Fixtures are static members added by extensions, so tests use implicit member syntax:

  ```k
  extend Product {
      public static let sample: Product = Product(id: …, name: "Mug", price: 9.99.usd)
  }
  ```

## Component tests

**Components are tested through accessibility queries**, the way a person using a screen reader finds
things:

```k
test AddToCart {
    use FakeCartService for CartService

    test "clicking add shows the new total" {
        let screen = render(AddToCart(product: .sample))

        screen.button(named: "Add to cart").click()

        expect screen.text("Total: $9.99").isVisible
    }
}
```

An element a test can't find by role and name isn't accessible either, so component tests double as
accessibility checks.

| Query | Finds |
|---|---|
| `button(named:)`, `link(named:)`, `heading(named:)` | an element by its accessible name |
| `textField(labeled:)`, `passwordField(labeled:)`, `checkbox(labeled:)`, `dropdown(labeled:)` | a form field by its label |
| `text(…)` | text content |
| `item(containing:)` | a list item containing some text |
| `buttons()`, `items()`, … | plural forms, returning a list |

Queries are named after K's element names, and every element handle supports the same queries, scoped
to its subtree: `screen.item(containing: "Buy milk").button(named: "Delete")`.

Handles have state (`exists`, `isVisible`, `isChecked`, `isDisabled`, `value`, `text`) and actions
(`click()`, `fill(text)`, `press(.enter)`). Synchronous updates are applied after every action. For async
work, `await screen.settle()` waits until no load, effect or handler is in flight.

## Time

`Date.now`, `2.hours.ago` and every `Timer` read an injected `Clock`. In tests, `TestClock` is installed
by default, starts at a fixed instant, and moves only when you say so:

```k
test CityWeather {
    use TestClock for Clock
    use FakeWeatherService for WeatherService

    @Inject let clock: TestClock
    @Inject let weather: FakeWeatherService

    test "the forecast refreshes every ten minutes" {
        let screen = render(CityWeather(placeId: Place.berlin.id))
        await screen.settle()

        clock.advance(by: 10.minutes)
        await screen.settle()

        expect weather.forecastCalls == 2
    }
}
```

No real time passes, so time-based tests are fast and never flaky.

## Integration tests

**Integration tests run the real stack in-process.** A test group without `use` rules gets the real
services, protocols called directly on their server implementations, real repositories, and a SQLite
database. **Each test runs inside a transaction that is rolled back afterwards.**

```k
test GuestbookService {
    @Inject let guestbook: GuestbookService

    test "a signed entry is listed" {
        await guestbook.sign(request: SignRequest(name: "Ana", message: "Hello!"))

        expect (await guestbook.entries()).count == 1
    }
}
```

Tests never read your environment. Configuration a test needs is bound in the group, for example
`use StripeWebhookConfig.sample for StripeWebhookConfig`.

## No snapshot tests

K has no snapshot tests. They encourage accepting changes without reviewing them. Write an `expect` for
the thing you actually care about.
