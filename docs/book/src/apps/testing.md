# Testing

Testing is part of the language. `test` and `expect` are keywords, tests sit next to the code they test,
and the same dependency-injection rules that build your app let tests replace any part of it.

<!-- The declarations the samples in this chapter use; checked, not rendered.

```k
// src/store/Product.k
import k.crypto.Uuid

/// A product.
public struct Product implements Codable {
    /// Its id.
    let id: Uuid
    /// Its name.
    let name: String
    /// Its price.
    let price: Money
    /// How many are in stock.
    let stock: Int = 10
}
```

```k
// src/store/Context.k
import k.crypto.Uuid
import k.rpc.RpcFailure

enum CartError implements Error {
    case outOfStock(available: Int)
    case rpcFailure(RpcFailure)
}

struct AddItemRequest implements Codable {
    let productId: Uuid
}

struct CartItem implements Codable {
    let price: Money
}

protocol CartProtocol {
    rpc add(request: AddItemRequest) throws CartError -> CartItem
}

service CartApi implements CartProtocol {
    rpc add(request: AddItemRequest) async throws CartError -> CartItem {
        return CartItem(price: 9.99.usd)
    }
}

service CartService {
    @Inject let api: CartProtocol

    @State var items: List<CartItem> = []

    var total: Money { items.sum(in: .usd) { item => item.price } }

    func add(product: Product) async throws CartError {
        guard product.stock > 0 else {
            throw CartError.outOfStock(available: 0)
        }
        items.append(try await api.add(request: AddItemRequest(productId: product.id)))
    }
}

component AddToCart {
    @Prop let product: Product
    @Inject let cart: CartService

    func add() async {
        do {
            try await cart.add(product: product)
        } catch _ {
        }
    }

    view {
        button { "Add to cart" }
            .on(click: add)

        text { "Total: {cart.total.formatted(as: .currency)}" }
    }
}
```

```k
// src/store/FakeCartApi.test.k
import k.crypto.Uuid

service FakeCartApi implements CartProtocol {
    rpc add(request: AddItemRequest) async throws CartError -> CartItem {
        return CartItem(price: 9.99.usd)
    }
}

service FakeCartService implements CartService {
    @State var items: List<CartItem> = []

    var total: Money { items.sum(in: .usd) { item => item.price } }

    func add(product: Product) async throws CartError {
        items.append(CartItem(price: product.price))
    }
}

extend Product {
    static let soldOut = Product(id: Uuid("1b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9b"), name: "Lamp", price: 20.00.usd, stock: 0)
}
```
-->

```k
// src/store/CartService.test.k: lives next to CartService.k, in the same module
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
- **A group's name is a free identifier.** By convention it is the declaration under test
  (`test CartService { }`), but it doesn't have to resolve to anything. A file may contain several
  groups, for example one per scenario that needs different `use` rules.
- **`test "description" { }` declares a single test.**
- **Every `test "…"` gets a fresh container,** so no service, fake, session or theme state leaks between
  tests.
- `k check` runs them after its other checks, so CI needs only `k check`. `k test` runs them alone, and
  `k test --watch` re-runs the affected ones on every save.

## `expect`

**`expect` takes any boolean expression.** There is no matcher library: no `toEqual`, no
`toHaveLength`. When an expectation fails, the test runner shows the value of each sub-expression:

```text
error[K7510]: expectation failed: cart.items.count == 1
  --> src/store/Cart.test.k:14:16
   |
14 |         expect cart.items.count == 1
   |                ^^^^^^^^^^^^^^^^^^^^^
   |
   = note: `cart.items.count` was 0
   = note: in test `Cart › adds a product`
```

Absence is checked with `not`, never by comparing with `false`:

```k
~import k.test.render
~
~component Todos {
~    view {
~        text { "Nothing to do" }
~    }
~}
~
~struct Todo {
~    let isDone: Bool
~}
~
~test Todos {
~    test "an empty list" {
~        let screen = render(Todos())
~        let todo = Todo(isDone: true)
expect not screen.button(named: "Clear completed").exists
expect todo.isDone
~    }
~}
```

`x == true` and `x == false` are reported with a fix to `x` and `not x`; the failure output shows the
operand's value either way.

**`expect throws` takes a pattern**, as `catch` does, so nested cases can be checked, and a case written
without its payload matches any payload:

```k,ignore
expect throws WeatherError.upstream(.serviceUnavailable) { … }
expect throws AccessError.rpcFailure(.rejected(.unauthenticated)) { … }
expect throws WeatherError.upstream { … }
```

## Test doubles and defaults

**In tests, every standard-library interface that does I/O outside the process has a hermetic default**
from `k.test`, which records calls and does no I/O. A test that forgets a fake therefore never reaches
the network, a mail server or real browser storage. The `Clock` service gets one too, so no test reads
the real time:

| Interface | Default in `k test` | What it does |
|---|---|---|
| `Clock` (a stdlib service) | `TestClock` | virtual time from a fixed instant, shared by the test's server and browser containers; `advance(by:)` fires due timers |
| `JobQueue` | `RecordingJobQueue` | stores jobs without running them; `payloads(of: OrderConfirmationJob.kind)` |
| `LocalStorage` | `MemoryStorage` | an empty in-memory store that can simulate failures |
| `EmailProvider` | `RecordingEmailProvider` | keeps sent messages in `sent`; `simulateFailure(error:)` |
| `HttpClient` | `TestHttpClient` | answers only requests it has a canned response for; any other request fails the test |

- **A test reaches a default with `@Inject`, by its concrete type:**
  `@Inject let clock: TestClock`, `@Inject let mailer: RecordingEmailProvider`,
  `@Inject let storage: MemoryStorage`.
- **`use` is written only to choose something else,** such as `use InlineJobQueue for JobQueue` or the
  app's own `use FakeHttpClient for HttpClient`. A `use` that restates the default, such as
  `use TestClock for Clock`, is a warning (`K4014`) with a quick fix that removes it.
- **Precedence:** a test group's `use` rules override the `App.k` rules for `.test`, which override these
  defaults.
- **Channels run in-process too,** through the real frames and handlers. `TestChannels` drops the
  test's connection, `TestPeers` signs a second person in, in another browser of the same test, and
  `TestChannel<C, S, E>` scripts a channel for a client service alone
  ([Channels](channels.md#testing)).

```k
~import k.text.EmailAddress
~import k.test.RecordingEmailProvider
// examples/shop/shop/src/store/OrderConfirmationJob.test.k
test OrderConfirmationJob {
    @Inject let job: OrderConfirmationJob
    @Inject let mailer: RecordingEmailProvider
    // ...
    test "the email names the item, the total and the receipt link" {
        let order = try await placeOrder()

        try await job.run(payload: OrderConfirmation(orderId: order.id))

        let email = mailer.sent[0]

        expect email.to == EmailAddress("ana@example.com")
        expect email.subject == "Your K Shop order is confirmed"
        // ...
    }
// ...
}
```

### Configs in tests

Tests never read environment variables. Standard-library configs have built-in test values (an
in-memory SQLite database, the origin `http://localhost`). **The app's own configs are bound by each test
group whose graph reaches them**, with a value:

```k
// examples/shop/shop/src/payments/StripeWebhookVerifier.test.k
test StripeWebhookVerifier {
    use StripeWebhookConfig.sample for StripeWebhookConfig

    @Inject let verifier: StripeWebhookVerifier
    // ...
}
```

The value may be a shared static member such as `StripeWebhookConfig.sample`, or the config's memberwise
initializer, which exists only in test code. A test group whose graph reaches an unbound app config,
through services, repositories or a protocol's server side, is a compile error (`K7503`) that names the path to it. See
[Configuration and secrets](config.md#tests).

## Fakes, fixtures and test-only modules

- Shared fakes and fixtures live in **test-only modules**, directories named `testing` that only test
  code can import, and that never reach a build.
- **Fixtures are static members added by public extensions** in a test-only module, so tests use implicit
  member syntax (`.berlin`, `.sample`). Public extensions in a test-only module are visible to every test
  file without an import, and to no production code:

  ```k
  // src/testing/Samples.k
  ~import k.crypto.Uuid
  ~import store.Product
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
~import k.test.render
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
accessibility checks. Queries are named after Kobra's element names, not ARIA roles, so a test reads like
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
| `screen.currentPage` | the `Page` the router is on, updated by `Router.navigate(to:)` and links |
| `await screen.settle()` | waits until no `@Load`, async effect or handler is in flight |
| `screen.unmount()` | unmounts the view, running every `onCleanup` |

- **`named:` matches the full accessible name exactly,** after whitespace normalisation (leading and
  trailing whitespace removed, inner runs collapsed to one space). `button(named: "Add")` doesn't find a
  button named "Add to cart". The same holds for every `named:` query. `item(containing:)`, by contrast,
  matches part of an item's text, as its label says.
- **A singular query returns a handle even when nothing matches.** `exists` and `isVisible` are then
  `false`. An action on a handle that matches nothing, or more than one element, fails the test with the
  query and what it found.
- **Every handle supports the same queries, scoped to its subtree:**
  `screen.item(containing: "Buy milk").button(named: "Delete")`. This finds one of many identically named
  controls without test ids.
- **Element state:** `exists`, `isVisible`, `isChecked`, `isPressed`, `isDisabled`, `value`, `text`,
  `level`, `isFocused`, and `wasScrolledIntoView` once code scrolled to it (`ElementRef`; the test DOM has
  no layout). `screen.focused` is the element of the view that has the focus. **Actions:** `click()`, `fill(text)`, `blur()` (leaves a field, which starts its `@Check`), `press(.enter)`, `press(.escape)` and the other `Key`
  cases. Pressing Enter in a form field submits the form, as in a browser.
- **Synchronous updates are flushed by every action,** so a test can `click()` and then `expect` with no
  `settle()`. `settle()` is needed only for async work. It doesn't wait for work that is waiting on the
  `TestClock`, or for the next element of an open stream.

**`render(SomePage(param: value))` renders a page** without its layout, exactly as the router would after
navigating to it: its `@Load` fields start at once, and `loading`, `view` or `failed` is shown as they
resolve. Guards still apply.

**A navigation shows the page it goes to,** inside its layouts, on the screen of the rendered page, as
the router would in a browser: the page that was shown is unmounted, and after `await screen.settle()`
the queries find the new one. Huddle's lobby creates a room and checks the room it opened:

```k
// examples/huddle/src/chat/Lobby.test.k
        screen.textField(labeled: "Room name").fill(" Team ")
        screen.button(named: "Create room").click()
        await screen.settle()

        // The screen shows the room it navigated to, inside the app's
        // layout, as the browser would.
        expect screen.currentPage == .room(name: "team")
        expect screen.heading(named: "#team").exists
        expect screen.button(named: "Sign out").exists
```

A test's screens share one browser, so one of them follows the URL: the newest rendered page still
mounted. A rendered component's screen keeps showing the component; `currentPage` still changes.

## Time

The current time and every timer come from the injected `Clock` service, and in tests the clock is
always a `TestClock`. It starts at a fixed instant and moves only when you say so:

```k
~import k.test.TestClock
~import k.test.render
// examples/weather/src/pages/CityWeather.test.k
test CityWeather {
    use FakeWeatherService for WeatherService

    @Inject let weather: FakeWeatherService
    @Inject let clock: TestClock
    // ...
    test "the forecast refreshes every ten minutes" {
        let screen = render(CityWeather(placeId: Place.berlin.id))
        await screen.settle()

        expect weather.forecastCalls == 1

        clock.advance(by: 9.minutes)
        await screen.settle()

        expect weather.forecastCalls == 1

        clock.advance(by: 1.minute)
        await screen.settle()

        expect weather.forecastCalls == 2
        // ...
    }
// ...
}
```

`advance(by:)` fires every timer and sleep that falls due, in order, and no real time passes, so
time-based tests are fast and never flaky. The test's server and browser containers share one virtual
time. See [Time and timers](time.md#testing-with-testclock).

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
  refusals and declared errors arrive as they would over the network. No network is involved, so
  `rpcFailure(.offline)` and `rpcFailure(.timeout)` never occur.
- **Every test has one session**, signed out at the start. The test acts as a user through
  `SessionManager`, without creating an account, and the browser's `Session` mirror follows:

```k
// examples/notes/src/notes/Notes.test.k
~import k.auth.SessionManager
~import k.test.TestClock
~import k.test.render
@Inject let sessions: SessionManager
@Inject let clock: TestClock

test "the list shows the account's notes and filters as you type" {
    await sessions.signIn(userId: AccountId.ada)
    try await notes.create(draft: draft(title: "Groceries", body: "Milk and eggs", tags: "home"))
    try await notes.create(draft: draft(title: "Ideas", tags: "work"))
    let screen = render(Notes())
    await screen.settle()

    expect screen.link(named: "Groceries").isVisible
    expect screen.link(named: "Ideas").isVisible
    // ...
}
```

## Integration tests and the database

**Integration tests use the app's real services, repositories and in-process protocols; external I/O is
always replaced** by the defaults above. **Every test gets a fresh in-memory SQLite database** with every
migration applied, data migrations included, so a test sees only what it wrote.

The app's transactions commit as they do in production, and the outermost `database.transaction`'s commit
fires the commit hooks, so `InlineJobQueue` runs jobs, and `Broadcast` publishes, exactly when they would
in production.

## HTTP-level tests: `TestClient`

`@Inject let client: TestClient` sends requests through the whole server stack in-process: routing,
guards, server rendering and endpoint pages, with no socket.

```k
// examples/shop/shop/src/webhooks/StripeWebhook.test.k
~import k.http.HttpRequest
~import k.http.HttpResponse
~import k.test.InlineJobQueue
~import k.server.JobQueue
~import k.server.PublicUrls
~import k.test.TestClient
test StripeWebhook {
    use StripeWebhookVerifier for StripeEvents
    use StripeWebhookConfig.sample for StripeWebhookConfig
    use InlineJobQueue for JobQueue

    @Inject let client: TestClient
    // ...
    @Inject let urls: PublicUrls
    // ...
    func deliver(payload: String, signature: String?) async -> HttpResponse {
        let headers: Map<String, String> = if let signature { ["stripe-signature": signature] } else { [:] }

        let request = HttpRequest(method: .post, url: urls.absolute(endpoint: .stripeWebhook))
            .headers(headers)
            .body(.text(payload))

        return await client.send(request: request)
    }
    // ...
    test "an unsigned request is refused and changes nothing" {
        let order = try await placeOrder()
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{order.id}", amountTotal: 4800)

        let response = await deliver(payload: payload, signature: nil)

        expect response.status == .badRequest
        expect await orders.byId(order.id)?.status == .awaitingPayment
        expect mailer.sent.isEmpty
    }
    // ...
}
```

- `send(request:)` takes an outgoing `HttpRequest`, which the runtime converts to the `IncomingRequest`
  a handler receives, as the wire would. It never throws. Response header names are lowercased.
- Requests use the test's server container, so its `use` rules apply, and the database is the test's
  own. The client keeps cookies across requests in a test, like one browser.
- `TestClient` renders view pages too, which is how a test checks what crawlers receive (`<title>`, the
  canonical link, the status set by `status(.notFound)`), since `render()` doesn't produce document
  metadata.

## No snapshot tests

Kobra has no snapshot tests. They encourage accepting changes without reviewing them. Write an `expect` for
the thing you actually care about. To look at a view in a chosen state, write a
[preview](previews.md) beside it instead: `k check` mounts it, but nothing compares its pixels.
