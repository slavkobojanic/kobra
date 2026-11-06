# Proposal: Test Doubles, Test-Only Modules and Screen Queries

> **Status: proposal.** Not yet accepted into the design. First used by `examples/weather`; extended by `examples/todos` and `examples/notes`.

## Problem

[21-testing.md](../21-testing.md) shows `use FakeCartService for CartService` and fixtures such as `.sample`. It doesn't say where fakes and fixtures live, how a fake conforms to a concrete service, how a test reaches the fake to configure it, or how a test waits for async loads to finish.

## Proposal

### Test-only modules

- **A directory named `testing`, at any depth, is a test-only module.** Only `*.test.k` files and other test-only modules may import from it. It's never part of `k build` output, and it may import `k.test`.
- Shared fakes and fixtures go there (`src/testing/FakeWeatherService.k`). Helpers used by a single test file stay in that file.
- **Test files don't take part in module cycle detection.** `weather/WeatherService.test.k` may import `testing.FakeHttpClient` even though `testing` imports `weather.Place`, because nothing imports a test file.
- **Declarations in a `*.test.k` file** (helper functions, wire-format builders) are visible only to the test files of the same module.

### Fakes of concrete services

- In test-only code, **`service FakeX implements X` is allowed when `X` is a service.** It conforms to `X`'s public API: every `public` function with the same signature, and every `public` `let` or `var` with the same type. A missing or mismatched member is a compile error that lists what's missing.
- Outside test-only code, `implements` a service is a compile error. Production code uses interfaces for polymorphism ([08-types.md](../08-types.md)).

### Reaching the fake

After `use FakeX for X`, a test group may write **`@Inject let x: FakeX`** to get the same instance the code under test receives. It can then configure the fake and inspect what was recorded.

```k
test CityWeather {
    use FakeWeatherService for WeatherService

    @Inject let weather: FakeWeatherService

    test "an unknown place shows a not-found message" {
        weather.simulateFailure(error: .placeNotFound(id: 42))
        …
    }
}
```

- **Every `test "…"` gets a fresh container,** so no fake state leaks between tests.

### Shared fixtures

Fixtures are static members added by extensions in a test-only module, so tests use implicit member syntax (`.berlin`, `.sample`) exactly as in [21-testing.md](../21-testing.md):

```k
extend Place {
    public static let berlin: Place = Place(id: 2950159, name: "Berlin", …)
}
```

**Public extensions in test-only modules apply to every test file** without an import, because an extension has no name to import.

### Rendering and querying

`render(view)` returns a `Screen`. In addition to the queries in [21-testing.md](../21-testing.md):

| Member | Meaning |
|---|---|
| `heading(named:)`, `link(named:)`, `button(named:)`, `textField(labeled:)`, `text(_)` | accessibility queries, returning an element handle |
| `element.isVisible` | whether the element exists and is visible |
| `element.click()`, `element.fill(text)` | user actions. `fill` replaces the field's value and fires input events. |
| `await screen.settle()` | waits until no `@Load`, async effect or event handler is in flight, apart from work waiting on a `TestClock` ([timers-and-clock.md](timers-and-clock.md)) |
| `screen.unmount()` | unmounts the view, running every `onCleanup` |

#### More queries, element state and scoping

*Extension first used by `examples/todos`.*

Queries are named after K's element names ([05-views.md](../05-views.md)), not ARIA role names, so a test reads like the view it checks:

| Member | Meaning |
|---|---|
| `checkbox(labeled:)` | a checkbox, found by its label ([element-accessibility.md](element-accessibility.md)) |
| `item(containing:)` | a list item whose text contains the given string |
| `navigation(named:)` | a `nav` landmark, found by its accessible name |
| `list()`, `alert()`, `status()` | the only element of that kind. `alert` and `status` are the live regions set with `.role(…)`. |
| `items()`, `buttons()`, … | **plural forms** of every query, returning a `List` of handles, possibly empty |
| `screen.title` | the document title from the `metadata` blocks in effect ([22-metadata.md](../22-metadata.md)) |

- **A singular query returns a handle even when nothing matches.** `exists` is then `false`, and reading `isVisible` gives `false`. An action (`click()`, `fill(…)`) on a handle that matches nothing, or matches more than one element, fails the test with the query and what it found. That makes `expect screen.button(named: "Clear completed").exists == false` the way to check absence.
- **Every handle supports the same queries, scoped to its subtree:** `screen.item(containing: "Buy milk").button(named: "Delete")`. This finds one of many identically named controls without inventing test ids.
- **Element state:** `exists`, `isVisible`, `isChecked`, `isPressed`, `isDisabled`, `value` (a field's current text), `text` (the element's text content), `level` (a heading's level).
- **More actions:** `press(.enter)`, `press(.escape)` and the other `Key` cases send a key press to the element. Pressing Enter in a form field submits the form, as in a browser.
- **Synchronous updates are flushed by every action,** so a test can `click()` and then `expect` with no `settle()`. `await screen.settle()` is needed only for async work.
- **Tests start from clean global state.** Each test renders into a fresh document, with the theme on `.base` ([theme-variants.md](theme-variants.md)).
- **A test file may declare helper components**, such as a host that holds `@State` and passes it to the component under test with `bind`. Like other test-file declarations, they are visible only to the module's test files.

#### Pages, navigation, streams and the full stack

*Extension first used by `examples/notes`.*

- **More field queries**, named after their elements like the rest: `passwordField(labeled:)`, `textArea(labeled:)`, `numberField(labeled:)`, `dropdown(labeled:)`.
- **`render(SomePage(param: value))` renders a page**, without its layout, exactly as the router would after navigating to it: its `@Load` fields start at once, and `loading`, `view` or `failed` is shown as they resolve. Guards still apply: rendering a page that `requires Authenticated` with no session shows the redirect, not the page.
- **`screen.currentPage: Page`** is the page the router is on. A `Router.navigate(to:)` from the rendered view updates it, so a test checks where a successful form went with `expect screen.currentPage == .notes`. The screen keeps showing the view it rendered; a test that needs the destination renders it separately.
- **Streams don't block `settle()`.** An async effect that is waiting for the next element of an open stream ([server-streaming.md](server-streaming.md)) counts as idle, once every element already delivered has been processed. So `settle()` returns for a page that follows a stream, and returns again after a change made elsewhere in the test has reached it.
- **Integration tests use no fakes.** A test group without `use` rules gets the real services, protocols called in-process, real repositories, and a SQLite database that is rolled back after each test ([21-testing.md](../21-testing.md)). The test acts as one browser with one `Session` ([auth-and-sessions.md](auth-and-sessions.md)).

### Expecting errors with payloads

`expect throws` takes a pattern, as `catch` does, so nested cases can be checked: `expect throws WeatherError.network(.timeout) { … }`. A case written without its payload (`WeatherError.network`) matches any payload.

### Configs, jobs and HTTP-level tests

*Extension first used by `examples/shop`.*

- **Configs are bound per test group with a value:** `use StripeWebhookConfig.sample for StripeWebhookConfig`. Tests never read environment variables ([config-and-secrets.md](config-and-secrets.md)).
- **`use InlineJobQueue for JobQueue`** runs enqueued background jobs as soon as their transaction commits, so a test sees their effects ([background-jobs.md](background-jobs.md)).
- **`@Inject let client: TestClient`** sends HTTP requests through the whole server stack in-process: endpoint pages, page guards and server-rendered HTML with its `metadata`. It shares the test's container and `Session` ([server-endpoints.md](server-endpoints.md)).

## Why

- A naming convention for test-only modules reuses "a directory is a module" and enforces the boundary at compile time, like the server/client split.
- Allowing `implements` of a service only in tests makes the existing `use FakeCartService for CartService` example precise, without adding service inheritance to production code.
- `settle()` with a virtual clock gives deterministic async tests with no arbitrary sleeps.
