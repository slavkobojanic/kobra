# Testing Kobra code

Sources: `book/apps/testing.md`, `book/reference/stdlib/k-test.md` and `design/21-testing.md`.
Every sample here passes `k test`.

## Shape

- `Name.test.k` sits next to the code, in the same module. `test Group { … }` holds `use X for Y`
  overrides, `@Inject` fields and helper functions, and `test "description" { … }` is one test.
- **Every test gets a fresh container**: new services, a new in-memory SQLite database with all
  migrations, a signed-out session, a `TestClock` at a fixed instant, and a `Theme` at `.base`.
- `expect <Bool>` prints the value of each sub-expression on failure. Use `expect not x`.
- `expect throws SomeError.case { try … }` takes a pattern. A case without its payload matches any
  payload, and nested patterns work (`RpcOnlyError.rpcFailure(.rejected(.unauthenticated))`).
- `k check` runs every test. Use `k test -f "Group › part"` while you iterate.
- There are no snapshot tests and no matchers.

## Test doubles you get for free

| Real | Test default (`import k.test.…`) | Useful members |
|---|---|---|
| `Clock` | `TestClock` | `advance(by:)` |
| `HttpClient` | `TestHttpClient` | `serveText(path:_:status:)`, `serveJson(path:_:)`, `respond(path:with:)`, `fail(path:error:)`, `requests` |
| `LocalStorage` | `MemoryStorage` | `writeRaw(name:text:)`, failure simulation |
| `EmailProvider` | `RecordingEmailProvider` | `sent`, `simulateFailure(error:)` |
| `JobQueue` | `RecordingJobQueue` (or `use InlineJobQueue for JobQueue`) | `payloads(of: Job.kind)` |
| `Collections` | `MemoryCollections` | `failWrites(error:)` |

Reach a default with `@Inject` by its **concrete type** (`@Inject let clock: TestClock`). A `use` that
restates the default is a warning (K4014). A request to `TestHttpClient` that has no canned answer
fails the test.

```k
import k.test.TestHttpClient

test ForecastService {
    @Inject let http: TestHttpClient
    @Inject let forecasts: ForecastService

    test "the temperature is decoded" {
        http.serveJson(path: "/v1/forecast", Forecast(temperature: 21.5))

        let forecast = try await forecasts.current(latitude: 59.9, longitude: 10.7)

        expect forecast.temperature == 21.5
    }

    test "a server error status is an upstream error" {
        http.serveText(path: "/v1/forecast", "Service unavailable", status: .serviceUnavailable)

        expect throws ForecastError.upstream(.serviceUnavailable) {
            try await forecasts.current(latitude: 59.9, longitude: 10.7)
        }
    }
}
```

(Note the `{{` for a literal brace in the JSON string.)

## Time

```k
import k.test.TestClock

test Reminder {
    @Inject let clock: TestClock
    @Inject let reminder: Reminder

    test "it rings when its time comes, not before" {
        reminder.start(after: 10.minutes)

        clock.advance(by: 9.minutes)
        expect not reminder.hasRung

        clock.advance(by: 1.minute)
        expect reminder.hasRung
    }
}
```

## Fakes and fixtures

Shared fakes and fixtures go in a `testing` directory (a test-only module). In test-only code,
`service FakeX implements X` is allowed even when `X` is a concrete service. It must match every
non-private member.

```k
import client.Forecast
import client.ForecastError
import client.ForecastService

/// Answers with a fixed temperature and counts the calls.
public service FakeForecastService implements ForecastService {
    @State var calls: Int = 0

    func current(latitude: Double, longitude: Double) async throws ForecastError -> Forecast {
        calls += 1
        return Forecast(temperature: 21.5)
    }
}
```

After `use FakeX for X`, inject `FakeX` to configure or inspect the same instance:

```k
import k.test.TestClock
import k.test.render
import testing.FakeForecastService

test CurrentTemperature {
    use FakeForecastService for ForecastService

    @Inject let forecasts: FakeForecastService
    @Inject let clock: TestClock

    test "it shows the temperature and refreshes every ten minutes" {
        let screen = render(CurrentTemperature())
        await screen.settle()

        expect screen.text("21.5°").isVisible
        expect forecasts.calls == 1

        clock.advance(by: 10.minutes)
        await screen.settle()

        expect forecasts.calls == 2
    }
}
```

Fixtures are static members on a `public extend` in the test-only module, and are visible in every
test file without an import:

```k
public extend Account {
    static func sample(createdAt: Date) -> Account {
        return Account(id: .ana, email: "ana@example.com", passwordHash: "unused", createdAt: createdAt)
    }
}
```

## Component and page tests

`render(Component(props))` or `render(SomePage(param: value))` mounts the view in the test's browser
container. A page renders without its layout, but its guards apply. Queries are accessibility queries
named after Kobra elements:

- `heading(named:)`, `button(named:)`, `link(named:)`, `text(_)`
- `textField(labeled:)`, `passwordField(labeled:)`, `textArea(labeled:)`, `numberField(labeled:)`,
  `dropdown(labeled:)`, `checkbox(labeled:)`
- `item(containing:)`, `navigation(named:)`, `list()`, `alert()`, `status()`, plural forms
  (`items()`), `screen.title` and `screen.currentPage`
- State: `exists`, `isVisible`, `isChecked`, `isPressed`, `isDisabled`, `isFocused`, `value`, `text`
- Actions: `click()`, `fill(text)`, `blur()`, `press(.enter)`
- `await screen.settle()` waits for loads, effects and handlers (but not for the `TestClock`), and
  `screen.unmount()` unmounts.

`named:` matches the whole accessible name exactly. Handles scope further queries:
`screen.item(containing: "Milk").button(named: "Delete")`. Synchronous updates are flushed by every
action.

```k
import k.test.render

test Guestbook {
    test "signing adds the entry to the page" {
        let screen = render(Guestbook())
        await screen.settle()

        expect screen.text("Nobody has signed yet. Be the first!").isVisible

        screen.textField(labeled: "Name").fill("Ana")
        screen.textArea(labeled: "Message").fill("Hello!")
        screen.button(named: "Sign the guestbook").click()
        await screen.settle()

        expect screen.heading(named: "Ana").exists
        expect not screen.alert().exists
    }
}
```

## Sessions, guards, the database

In a group, `@Inject` resolves server-only declarations (repositories, `SessionManager`) from the
server container. Everything else, protocols included, comes from the browser container, so a
protocol is called as the browser would call it.

```k
test "the protocol requires a signed-in session" {
    expect throws RpcOnlyError.rpcFailure(.rejected(.unauthenticated)) {
        try await api.recent()
    }
}

test "a signed-in customer sees their orders" {
    let account = Account.sample(createdAt: clock.now)
    try await accounts.insert(account)
    await orders.insert(Order(id: Uuid.random(), account: account, total: 9.99.usd, placedAt: clock.now))
    await sessions.signIn(userId: AccountId.ana)

    let recent = try await api.recent()

    expect recent.count == 1
    expect not recent[0].isPaid
}
```

Act as a user with `await sessions.signIn(userId: …)`, which needs no real account. A second
person in channel tests is `TestPeers` (`book/apps/channels.md`).

## Configs and HTTP-level tests

A group whose graph reaches an app config must bind it (K7503). `TestClient` sends whole HTTP requests
through routing, guards, server rendering and endpoint pages, in-process:

```k
test PaymentWebhook {
    use WebhookConfig.sample for WebhookConfig
    use InlineJobQueue for JobQueue

    @Inject let client: TestClient
    @Inject let urls: PublicUrls
    @Inject let accounts: AccountRepository
    @Inject let orders: OrderRepository
    @Inject let mailer: RecordingEmailProvider
    @Inject let clock: TestClock

    func deliver(orderId: Uuid, token: String) async -> HttpResponse {
        let request = HttpRequest(method: .post, url: urls.absolute(endpoint: .paymentWebhook(orderId: orderId)))
            .headers(["x-webhook-token": token])

        return await client.send(request: request)
    }

    // …
}
```

`client.send(request:)` never throws, and lowercases response header names. Cookies persist across one
test's requests.
