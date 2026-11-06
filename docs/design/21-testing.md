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
        expect not screen.alert().exists
    }
}
```

## Rules

- **Test files sit next to the source as `Name.test.k`,** in the same module, so they can test package-private code.
- **`test` is a keyword.** `test Name { }` groups tests and holds DI overrides (`use X for Y`, the same syntax as `App.k`). `test "description" { }` declares a single test.
- **A group's name is a free identifier.** By convention it is the declaration under test (`test CartService { }`), but it doesn't have to resolve to anything. A file may contain several groups, for example one per scenario that needs different `use` rules.
- **`expect` is a keyword that takes any boolean expression.** There is no matcher library (`toEqual`, `toHaveLength`). On failure, the compiler shows the value of each sub-expression, for example: `cart.items.count` was `0`, expected `== 1`. Absence is checked with `not` (`expect not x.exists`); comparing a `Bool` with `true` or `false` is a lint ([08-types.md](08-types.md#booleans)).
- **`expect throws` takes a pattern,** as `catch` does: `expect throws WeatherError.upstream(.serviceUnavailable) { … }`. A case written without its payload (`WeatherError.upstream`) matches any payload.
- **Components are tested through accessibility queries:** `button(named:)`, `textField(labeled:)`, `text(...)`. An element a test can't find by role and name isn't accessible either.
- **Every `test "…"` gets a fresh container,** so no service, fake, session or theme state leaks between tests.
- **There are no snapshot tests.** They encourage accepting changes without reviewing them.
- `k test` runs tests. **`k test --watch`** runs them, then re-runs the affected ones whenever a source file is saved; "affected" comes from the compiler's incremental queries. `k check` runs every test in CI and has no watch mode.

## Fakes, fixtures and test-only modules

- Shared fakes and fixtures live in **test-only modules**, directories named `testing` that only test code can import ([16-modules.md](16-modules.md#test-only-modules)).
- **Fixtures are static members added by public extensions** in a test-only module, so tests use implicit member syntax (`.berlin`, `.sample`). Public extensions in a test-only module are visible to every test file without an import, by the ordinary extension rule ([16-modules.md](16-modules.md#extensions)).
- **A test file may declare helpers,** including helper components such as a host that holds `@State` and passes it to the component under test with `bind`. They are visible only to the module's test files.

### Fakes of concrete services

- In test-only code, **`service FakeX implements X` is allowed when `X` is a service.** It conforms to **every non-private member of `X`**: functions with the same signature, and fields (`let`, `var`, `@State`, `@Derived`) with the same type. Injected fields are always private ([16-modules.md](16-modules.md)), so a fake never has to declare its target's dependencies. A missing or mismatched member is a compile error that lists what's missing.
- Outside test-only code, `implements` a service is a compile error. Production code uses interfaces for polymorphism ([08-types.md](08-types.md)).
- **After `use FakeX for X`, a test group may write `@Inject let x: FakeX`** to get the same instance the code under test receives, to configure it and inspect what it recorded. The fake's fields are writable only by its own methods, like any service's ([10-services.md](10-services.md#state-ownership)).

## Test doubles and defaults

**In `.test`, every stdlib interface whose production implementation does I/O outside the process has a hermetic default** from `k.test` that records calls and does no I/O. A test that forgets a fake therefore never reaches the network, a mail server or real browser storage. The `Clock` service gets one too, so no test reads the real time.

| Interface | Default in `k test` | What it does |
|---|---|---|
| `Clock` (a stdlib service) | `TestClock` | virtual time from a fixed instant, shared by the test's server and browser containers; `advance(by:)` fires due timers ([35-time.md](35-time.md#testing-with-testclock)) |
| `JobQueue` | `RecordingJobQueue` | stores jobs without running them; `payloads(of: OrderConfirmationJob.kind)` ([30-background-jobs.md](30-background-jobs.md)) |
| `LocalStorage` | `MemoryStorage` | an empty in-memory store that can simulate failures ([36-local-storage.md](36-local-storage.md)) |
| `EmailProvider` | `RecordingEmailProvider` | keeps sent messages in `sent`; `simulateFailure(error:)` ([31-email.md](31-email.md)) |
| `HttpClient` | `TestHttpClient` | answers only requests it has a canned response for; any other request fails the test, naming the URL ([32-http-client.md](32-http-client.md)) |

- **A test reaches a default with `@Inject`,** by its concrete type: `@Inject let clock: TestClock`, `@Inject let mailer: RecordingEmailProvider`.
- **`use` is written only to choose something else,** such as `use InlineJobQueue for JobQueue` or the app's own `use FakeHttpClient for HttpClient`. A `use` that restates the default is a warning with a quick fix that removes it, so there is one way to write a test.
- **Precedence:** a test group's `use` rules override the `App.k` rules for `.test`, which override these defaults.
- **Configs:** stdlib configs have built-in test values (an in-memory SQLite database, the origin `http://localhost`). The app's own configs must be bound by each test group whose graph reaches them, with a value: `use StripeWebhookConfig.sample for StripeWebhookConfig` ([25-config-and-secrets.md](25-config-and-secrets.md#tests)). Tests never read environment variables.

## Which side a test runs on

A test is one browser talking to one server, in one process:

- **Each test has a server container and a browser container.** In a test group, `@Inject` resolves server-only declarations (repositories, `SessionManager`, server-side services) from the server container, and every other service from the browser container. A protocol injected by a test is the browser container's client, so the test calls it as the browser would.
- **`render(…)` mounts the view in the test's browser container,** so the rendered view and the test share client service instances. Client-only services (`LocalStorage`, `Theme`) exist only there.
- **Protocol calls from the browser container go to the server implementations in-process, through the real wire format.** Requests and responses are encoded and decoded, validation runs, and guard refusals and declared errors arrive as they would over the network ([28-rpc-wire-format.md](28-rpc-wire-format.md)).
- **Every test has one session.** The test's `SessionManager` acts on it (`await sessions.signIn(userId: AccountId.sample)` to act as a user without creating an account), and the browser container's `Session` mirror is synchronized from it after every test statement and every rpc ([26-auth-and-sessions.md](26-auth-and-sessions.md#tests)). Guards run exactly as in production.

### Integration tests and the database

- **Integration tests use the app's real services, repositories and in-process protocols; external I/O is always replaced** by the defaults above.
- Every `k test` run uses a fresh in-memory SQLite database with every migration applied, data migrations included ([19-persistence.md](19-persistence.md#data-migrations)).
- **Each test runs inside a transaction the app can't see,** rolled back afterwards. The app's outermost `database.transaction` becomes a savepoint inside it, and its commit releases the savepoint and fires the commit hooks. So `InlineJobQueue` runs jobs, and `Broadcast` publishes, exactly when they would in production ([19-persistence.md](19-persistence.md#database-and-transactions)).

## Rendering and querying

`render(view)` returns a `Screen`.

- **`render(SomePage(param: value))` renders a page** without its layout, exactly as the router would after navigating to it: its `@Load` fields start at once, and `loading`, `view` or `failed` is shown as they resolve. Guards still apply: rendering a page that `requires Authenticated` with no session shows the redirect, not the page.
- Queries are named after K's element names ([05-views.md](05-views.md)), not ARIA role names, so a test reads like the view it checks.

| Member | Meaning |
|---|---|
| `heading(named:)`, `link(named:)`, `button(named:)`, `text(_)` | accessibility queries, returning an element handle |
| `textField(labeled:)`, `passwordField(labeled:)`, `textArea(labeled:)`, `numberField(labeled:)`, `dropdown(labeled:)`, `checkbox(labeled:)` | form controls, found by their label |
| `item(containing:)` | a list item whose text contains the given string |
| `navigation(named:)` | a `nav` landmark, found by its accessible name |
| `list()`, `alert()`, `status()` | the only element of that kind. `alert` and `status` are the live regions set with `.role(…)`. |
| `items()`, `buttons()`, … | plural forms of every query, returning a `List` of handles, possibly empty |
| `screen.title` | the document title from the `metadata` blocks in effect |
| `screen.currentPage: Page` | the page the router is on. A `Router.navigate(to:)` from the rendered view updates it; the screen keeps showing the view it rendered. |
| `await screen.settle()` | waits until no `@Load`, async effect or event handler is in flight, apart from work waiting on the `TestClock` or on the next element of an open stream |
| `screen.unmount()` | unmounts the view, running every `onCleanup` |

- **`named:` matches the full accessible name exactly,** after whitespace normalisation (leading and trailing whitespace removed, inner runs collapsed to one space). `button(named: "Add")` doesn't find a button named "Add to cart". The same holds for every `named:` query. `item(containing:)`, by contrast, matches part of an item's text, as its label says.
- **A singular query returns a handle even when nothing matches.** `exists` and `isVisible` are then `false`. An action on a handle that matches nothing, or more than one element, fails the test with the query and what it found.
- **Every handle supports the same queries, scoped to its subtree:** `screen.item(containing: "Buy milk").button(named: "Delete")`. This finds one of many identically named controls without test ids.
- **Element state:** `exists`, `isVisible`, `isChecked`, `isPressed`, `isDisabled`, `value` (a field's current text), `text` (the element's text content), `level` (a heading's level).
- **Actions:** `click()`, `fill(text)` (replaces a field's value and fires input events), `press(.enter)`, `press(.escape)` and the other `Key` cases. Pressing Enter in a form field submits the form, as in a browser.
- **Synchronous updates are flushed by every action,** so a test can `click()` and then `expect` with no `settle()`. `settle()` is needed only for async work.
- **Streams don't block `settle()`.** An async effect waiting for the next element of an open stream counts as idle once every element already delivered has been processed ([29-server-streaming.md](29-server-streaming.md#tests)).

## HTTP-level tests: `TestClient`

`@Inject let client: TestClient` sends requests through the whole server stack in-process: routing, guards, server rendering and endpoint pages, with no socket.

```k
test StripeWebhook {
    use StripeWebhookConfig.sample for StripeWebhookConfig

    @Inject let client: TestClient
    @Inject let urls: PublicUrls

    test "an unsigned request is refused" {
        let request = HttpRequest(method: .post, url: urls.absolute(endpoint: .stripeWebhook))
            .body(.text(payload))

        let response = await client.send(request: request)

        expect response.status == .badRequest
    }
}
```

- `send(request:)` takes an outgoing `HttpRequest`, which the runtime converts to the `IncomingRequest` a handler receives, as the wire would ([27-endpoint-pages.md](27-endpoint-pages.md)). It never throws; transport failures can't happen in-process. Response header names are lowercased.
- **Requests use the test's server container,** so its `use` rules apply, and the database is the test's rolled-back one.
- **The client keeps cookies across requests in a test,** like one browser, and shares the test's session.
- `TestClient` renders view pages too, which is how a test checks what crawlers receive (`<title>`, the canonical link, structured data, the status set by `status(.notFound)`), since `render()` doesn't produce document metadata.

## Why

- A naming convention for test-only modules reuses "a directory is a module" and enforces the boundary at compile time, like the server/client split.
- Allowing `implements` of a service only in tests makes `use FakeCartService for CartService` precise, without adding service inheritance to production code.
- Hermetic defaults make the safe test the one that is written by default, following the config precedent: only the app's own choices need binding.
- Routing in-process calls through the real wire format means a type that can't be encoded, or a validation rule, fails in the test rather than in production.
- `settle()` with a virtual clock gives deterministic async tests with no arbitrary sleeps.
