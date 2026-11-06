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
- **Every `test "…"` gets a fresh container,** so no service, fake, session or theme state leaks between
  tests.
- `k test` runs them, `k test --watch` re-runs the affected ones on every save, and `k check` runs them
  all in CI.

## `expect`

**`expect` takes any boolean expression.** There is no matcher library: no `toEqual`, no
`toHaveLength`. When an expectation fails, the test runner shows the value of each sub-expression:

```text
expect cart.items.count == 1
       cart.items.count was 0, expected == 1
```

Absence is checked with `not`, never by comparing with `false`:

```k
expect not screen.button(named: "Clear completed").exists
expect todo.isDone
```

`x == true` and `x == false` are reported with a fix to `x` and `not x`; the failure output shows the
operand's value either way.

**`expect throws` takes a pattern**, as `catch` does, so nested cases can be checked, and a case written
without its payload matches any payload:

```k
expect throws WeatherError.network(.timeout) { … }
expect throws AccessError.rpcFailure(.rejected(.unauthenticated)) { … }
expect throws WeatherError.network { … }
```

## Test doubles and defaults

**In tests, every standard-library interface that does I/O outside the process has a hermetic default**
from `k.test`, which records calls and does no I/O. A test that forgets a fake therefore never reaches
the network, a mail server or real browser storage:

| Interface | Default in `k test` | What it does |
|---|---|---|
| `Clock` | `TestClock` | virtual time from a fixed instant; `advance(by:)` fires due timers |
| `JobQueue` | `RecordingJobQueue` | stores jobs without running them; `payloads(of: OrderConfirmationJob.kind)` |
| `LocalStorage` | `MemoryStorage` | an empty in-memory store that can simulate failures |
| `EmailProvider` | `RecordingEmailProvider` | keeps sent messages in `sent`; `simulateFailure(error:)` |
| `HttpClient` | `TestHttpClient` | answers only requests it has a canned response for; any other request fails the test |

- **A test reaches a default with `@Inject`, by its concrete type:**
  `@Inject let clock: TestClock`, `@Inject let mailer: RecordingEmailProvider`,
  `@Inject let storage: MemoryStorage`.
- **`use` is written only to choose something else,** such as `use InlineJobQueue for JobQueue` or the
  app's own `use FakeHttpClient for HttpClient`. A `use` that restates the default, such as
  `use TestClock for Clock`, is a warning with a quick fix that removes it.
- **Precedence:** a test group's `use` rules override the `App.k` rules for `.test`, which override these
  defaults.

```k
test OrderConfirmationJob {
    @Inject let job: OrderConfirmationJob
    @Inject let mailer: RecordingEmailProvider

    test "a paid order gets a confirmation" {
        try await job.run(payload: OrderConfirmation(orderId: Order.paid.id))

        expect mailer.sent.count == 1
    }
}
```

### Configs in tests

Tests never read environment variables. Standard-library configs have built-in test values (an
in-memory SQLite database, the origin `http://localhost`). **The app's own configs are bound by each test
group whose graph reaches them**, with a value:

```k
test StripeWebhookVerifier {
    use StripeWebhookConfig.sample for StripeWebhookConfig

    @Inject let verifier: StripeWebhookVerifier
}
```

The value may be a shared static member such as `StripeWebhookConfig.sample`, or the config's memberwise
initializer, which exists only in test code. A test group whose graph reaches an unbound app config is a
compile error, with a quick fix that adds the `use` line. See
[Configuration and secrets](config.md#tests).

## Fakes, fixtures and test-only modules

- Shared fakes and fixtures live in **test-only modules**, directories named `testing` that only test
  code can import, and that never reach a build.
- **Fixtures are static members added by public extensions** in a test-only module, so tests use implicit
  member syntax (`.berlin`, `.sample`). Public extensions in a test-only module are visible to every test
  file without an import, and to no production code:

  ```k
  // src/testing/Samples.k
  public extend Product {
      static let sample = Product(id: Uuid("0b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9b"), name: "Mug", price: 9.99.usd)
  }
  ```

- **A test file may declare helpers,** including helper components, such as a host that holds `@State`
  and passes it to the component under test with `bind`. They are visible only to the module's test
  files.

### Fakes of concrete services

- In test-only code, **`service FakeX implements X` is allowed when `X` is a service.** It conforms to
  every non-private member of `X`: functions with the same signature, and fields with the same type.
  Injected fields are always private, so a fake never has to declare its target's dependencies. A missing
  or mismatched member is a compile error that lists what's missing.
- Outside test-only code, `implements` a service is a compile error; production code uses interfaces for
  polymorphism.
- **After `use FakeX for X`, a test group may write `@Inject let x: FakeX`** to get the same instance the
  code under test receives, to configure it and inspect what it recorded. The fake's fields are writable
  only by its own methods, like any service's.

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
        expect not screen.alert().exists
    }
}
```

An element a test can't find by role and name isn't accessible either, so component tests double as
accessibility checks. Queries are named after K's element names, not ARIA roles, so a test reads like
the view it checks:

| Member | Meaning |
|---|---|
| `heading(named:)`, `link(named:)`, `button(named:)`, `text(_)` | an element by its accessible name or text |
| `textField(labeled:)`, `passwordField(labeled:)`, `textArea(labeled:)`, `numberField(labeled:)`, `dropdown(labeled:)`, `checkbox(labeled:)` | a form control, by its label |
| `item(containing:)` | a list item whose text contains the given string |
| `navigation(named:)` | a `nav` landmark, by its accessible name |
| `list()`, `alert()`, `status()` | the only element of that kind; `alert` and `status` are live regions |
| `items()`, `buttons()`, … | plural forms, returning a possibly empty `List` |
| `screen.title` | the document title from the `metadata` blocks in effect |
| `screen.currentPage` | the `Page` the router is on, updated by `Router.navigate(to:)` |
| `await screen.settle()` | waits until no `@Load`, async effect or handler is in flight |
| `screen.unmount()` | unmounts the view, running every `onCleanup` |

- **A singular query returns a handle even when nothing matches.** `exists` and `isVisible` are then
  `false`. An action on a handle that matches nothing, or more than one element, fails the test with the
  query and what it found.
- **Every handle supports the same queries, scoped to its subtree:**
  `screen.item(containing: "Buy milk").button(named: "Delete")`. This finds one of many identically named
  controls without test ids.
- **Element state:** `exists`, `isVisible`, `isChecked`, `isPressed`, `isDisabled`, `value`, `text`,
  `level`. **Actions:** `click()`, `fill(text)`, `press(.enter)`, `press(.escape)` and the other `Key`
  cases. Pressing Enter in a form field submits the form, as in a browser.
- **Synchronous updates are flushed by every action,** so a test can `click()` and then `expect` with no
  `settle()`. `settle()` is needed only for async work. It doesn't wait for work that is waiting on the
  `TestClock`, or for the next element of an open stream.

**`render(SomePage(param: value))` renders a page** without its layout, exactly as the router would after
navigating to it: its `@Load` fields start at once, and `loading`, `view` or `failed` is shown as they
resolve. Guards still apply.

## Time

`Date.now`, `2.hours.ago` and every `Timer` read the clock, and in tests the clock is always a
`TestClock`. It starts at a fixed instant and moves only when you say so:

```k
test CityWeather {
    use FakeWeatherService for WeatherService

    @Inject let weather: FakeWeatherService
    @Inject let clock: TestClock

    test "the forecast refreshes every ten minutes" {
        let screen = render(CityWeather(placeId: Place.berlin.id))
        await screen.settle()

        clock.advance(by: 10.minutes)
        await screen.settle()

        expect weather.forecastCalls == 2
    }
}
```

`advance(by:)` fires every timer and sleep that falls due, in order, and no real time passes, so
time-based tests are fast and never flaky. See [Time and timers](time.md).

## Which side a test runs on

A test is one browser talking to one server, in one process:

- **Each test has a server container and a browser container.** In a test group, `@Inject` resolves
  server-only declarations (repositories, `SessionManager`, server-side services) from the server
  container, and every other service from the browser container. A protocol injected by a test is the
  browser's client, so the test calls it as the browser would.
- **`render(…)` mounts the view in the browser container,** so the rendered view and the test share
  client service instances. Client-only services such as `LocalStorage` and `Theme` exist only there.
- **Protocol calls from the browser container go to the server implementations in-process, through the
  real wire format.** Requests and responses are encoded and decoded, validation runs, and guard
  refusals and declared errors arrive as they would over the network.
- **Every test has one session**, signed out at the start. The test acts as a user through
  `SessionManager`, without creating an account, and the browser's `Session` mirror follows:

```k
test NotesApi {
    @Inject let sessions: SessionManager
    @Inject let notes: NotesProtocol

    test "a guest can't list notes" {
        expect throws AccessError.rpcFailure(.rejected(.unauthenticated)) {
            try await notes.list()
        }
    }

    test "a signed-in user sees their notes" {
        await sessions.signIn(userId: AccountId.ada)

        let listed = try await notes.list()

        expect listed.isEmpty
    }
}
```

## Integration tests and the database

**Integration tests use the app's real services, repositories and in-process protocols; external I/O is
always replaced** by the defaults above. Every `k test` run uses a fresh in-memory SQLite database with
every migration applied, data migrations included.

**Each test runs inside a transaction the app can't see,** rolled back afterwards. The app's outermost
`database.transaction` becomes a savepoint inside it, and its commit fires the commit hooks, so
`InlineJobQueue` runs jobs, and `Broadcast` publishes, exactly when they would in production.

## HTTP-level tests: `TestClient`

`@Inject let client: TestClient` sends requests through the whole server stack in-process: routing,
guards, server rendering and endpoint pages, with no socket.

```k
test StripeWebhook {
    use StripeWebhookConfig.sample for StripeWebhookConfig

    @Inject let client: TestClient
    @Inject let urls: PublicUrls

    test "an unsigned request is refused" {
        let request = HttpRequest(method: .post, url: urls.absolute(endpoint: .stripeWebhook))
            .body(.text("{{}"))

        let response = await client.send(request: request)

        expect response.status == .badRequest
    }
}
```

- `send(request:)` takes an outgoing `HttpRequest`, which the runtime converts to the `IncomingRequest`
  a handler receives, as the wire would. It never throws. Response header names are lowercased.
- Requests use the test's server container, so its `use` rules apply, and the database is the test's
  rolled-back one. The client keeps cookies across requests in a test, like one browser.
- `TestClient` renders view pages too, which is how a test checks what crawlers receive (`<title>`, the
  canonical link, the status set by `status(.notFound)`), since `render()` doesn't produce document
  metadata.

## No snapshot tests

K has no snapshot tests. They encourage accepting changes without reviewing them. Write an `expect` for
the thing you actually care about.
