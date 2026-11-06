# Todos

A small todo list written entirely in K. You can add, rename, complete and delete todos, filter the list, clear completed todos, and switch between light and dark themes. Everything is saved in the browser's local storage, so a reload brings the list back.

The app is deliberately small and client-only: there is no server code, no RPC and no database. It exists to show K's component model, reactivity, styling and testing in one place. For a full-stack example that loads data over the network, see [`examples/weather`](../weather).

> K's compiler doesn't exist yet, so this code has been written by hand against the design in [`docs/design`](../../docs/design). Where the design leaves a gap, the example follows a proposal in [`docs/design/proposals`](../../docs/design/proposals) (see [Proposals this example relies on](#proposals-this-example-relies-on)).

## Running it

From this directory:

| Command | What it does |
|---|---|
| `k dev` | Starts the dev server with state-preserving hot module replacement. Edit a component and its `@State`, such as a half-typed todo, survives the reload. |
| `k build` | Builds the app. Because it renders on the client and has no server-only code, the output is static files in `dist/public` that any static host can serve. |
| `k test` | Runs every `*.test.k` file. Add `--watch` to re-run on change. |
| `k check` | Type checks, verifies formatting, runs the tests and checks lockfiles, as CI does. |
| `k fmt` | Formats the code in the one canonical style. |

## Structure

```
todos/
├── k.toml                          project manifest
├── App.k                           the app declaration: client rendering, app-wide metadata
└── src/
    ├── todo/                       module `todo`: the domain and its state
    │   ├── Todo.k                  struct Todo
    │   ├── TodoId.k                struct TodoId
    │   ├── Filter.k                enum Filter
    │   ├── TitleError.k            enum TitleError (the typed error for bad titles)
    │   ├── StorageProblem.k        enum StorageProblem (what to tell the user)
    │   ├── TodoService.k           service TodoService (the shared todo state)
    │   └── TodoService.test.k
    ├── appearance/                 module `appearance`: the color scheme choice
    │   ├── ColorScheme.k           enum ColorScheme
    │   ├── AppearanceService.k     service AppearanceService
    │   └── AppearanceService.test.k
    ├── ui/                         module `ui`: app-agnostic building blocks
    │   ├── TodoTheme.k             theme TodoTheme, with a dark variant
    │   ├── Panel.k                 component Panel (slots and @Stylable)
    │   └── Panel.test.k
    └── home/                       module `home`: the page and its parts
        ├── Home.k                  page Home at "/"
        ├── Home.test.k             end-to-end tests of the whole page
        ├── NewTodoField.k          + NewTodoField.test.k
        ├── TodoList.k              + TodoList.test.k
        ├── TodoRow.k               + TodoRow.test.k
        ├── FilterBar.k             + FilterBar.test.k
        ├── StorageNotice.k
        └── ThemeToggle.k
```

The layout follows the module rules in [16-modules.md](../../docs/design/16-modules.md):

- **Each directory is a module**, and the dependencies only point one way: `home` → `todo`, `appearance`, `ui`. Nothing imports `home`, and `todo` imports nothing from the app.
- **Each file holds one declaration, named after the file.** Only declarations used by another module are `public`. The components in `home` are used only by `Home`, so they have no modifier and stay private to the module. `Home` is public because the app's route table refers to it.
- **Tests sit next to the code they test** as `Name.test.k`, in the same module, so they can test module-private components directly. `StorageNotice` and `ThemeToggle` are small enough that `Home.test.k` covers them through the page.

## How it works

**State lives in services.** `TodoService` holds the list and is the only code that can change it: components read `todos.items` and the derived counts, and call methods such as `add(title:)` and `setDone(id:isDone:)`. `AppearanceService` does the same for the color scheme. The page itself owns just one piece of state, the selected `filter`, because it belongs to the page rather than the app.

**Every change is saved.** Each `TodoService` method checks its input, changes `items` in place (fine-grained, so only the affected row updates), and writes the whole list to `LocalStorage` under a typed `StorageKey<List<Todo>>`. On startup, the service's `init()` restores the saved list.

**Failures are values, not surprises.** Invalid titles throw a typed `TitleError`, which the components catch and show next to the field. Storage failures (data that no longer decodes, blocked storage, a full quota) never stop the app: the change still applies in memory, and `TodoService.problem` drives a `StorageNotice` that tells the user what is and isn't being saved.

**Rendering is client-only.** Local storage exists only in the browser, so `App.k` declares `@Rendering(.client)`. Otherwise a server-rendered page would show an empty list and then jump to the saved one.

## What each file demonstrates

| File | Language features |
|---|---|
| [`App.k`](App.k) | The `app` declaration, the `@Rendering(.client)` annotation, an app-wide `metadata` block, and why no `use … for …` rules are needed |
| [`src/todo/Todo.k`](src/todo/Todo.k), [`TodoId.k`](src/todo/TodoId.k) | Structs with value semantics, `let` vs `var` fields, derived conformances (`implements Codable, Equatable, Hashable`), a wrapper type for ids |
| [`src/todo/Filter.k`](src/todo/Filter.k) | An enum with a method, `match` as an exhaustive expression, `match self` |
| [`src/todo/TitleError.k`](src/todo/TitleError.k) | A typed error enum with an associated value (`tooLong(limit:)`), binding it in a pattern (`case .tooLong(let limit)`), a computed property |
| [`src/todo/TodoService.k`](src/todo/TodoService.k) | A `service` as the container for shared state, `@Inject`, `@State` and `@Derived` in a service, `init()`, typed `throws`, `try`, `guard … else`, `do`/`catch` that is exhaustive over each error enum, nested `@State` mutation (`items[index].isDone = …`), typed persistence with `k.storage`, `private` helpers, doc comments with `- Parameter`/`- Throws` sections |
| [`src/appearance/AppearanceService.k`](src/appearance/AppearanceService.k) | A second small service, `self.` to tell a field from a parameter, turning a failure into a value with `try … else { return }` when it is harmless |
| [`src/ui/TodoTheme.k`](src/ui/TodoTheme.k) | A typed `theme` with color, spacing, radius, font size and font tokens, and a `variant dark` that overrides only colors |
| [`src/ui/Panel.k`](src/ui/Panel.k) | A required trailing slot (`content`), optional chained slots (`toolbar`, `actions`) unwrapped with `if let`, an optional prop with a default (`.headingLevel(1)`), `@Stylable` with a class list (`[.panel, container]`), `@media` in a scoped style |
| [`src/home/Home.k`](src/home/Home.k) | A `page` with a named URL (`at "/"`), `@Derived` values built with `if` and `match` expressions (there are no ternaries), an `@Effect` that applies the theme variant, reactive `metadata`, `bind` to a child's `@Binding`, a scoped slot used from the caller (`.row { todo => … }`), passing a class to a child's `@Stylable` hook (`.container(.board)`), `if let` in a view |
| [`src/home/NewTodoField.k`](src/home/NewTodoField.k) | `@State` with `bind` to a built-in `textField`, form submission, `catch let` to keep a typed error as state, chained modifiers each on their own line, theme colors in styles (`.primary.opacity(0.85)`) |
| [`src/home/TodoList.k`](src/home/TodoList.k) | A scoped slot (`row: (Todo) -> View`) next to a trailing slot (`empty`), `if`/`else` in a view, `for … in … key …` |
| [`src/home/TodoRow.k`](src/home/TodoRow.k) | Local editing state, `if`/`else` between two layouts, a custom `Binding(get:set:)` passed to a `checkbox`, a function passed by name (`set: markDone`), conditional classes with `when` (`.done when todo.isDone`), `.disabled` driven by `@Derived` |
| [`src/home/FilterBar.k`](src/home/FilterBar.k) | `@Binding` written by the child (`selection = filter`), `for … key` over enum values, `match` in a function, `when` classes, the `pressed` accessibility state, a labelled `nav` landmark |
| [`src/home/StorageNotice.k`](src/home/StorageNotice.k) | `match` in a view, with one exhaustive branch per enum case, and a live region (`.role(.status)`) |
| [`src/home/ThemeToggle.k`](src/home/ThemeToggle.k) | A component with no `style` block, and a custom `Binding` that routes writes through a service method |

## The tests

The tests use `test`/`expect`, `use … for …` overrides and accessibility queries ([21-testing.md](../../docs/design/21-testing.md)). There are no snapshot tests, and no test ids: if a test can't find an element by its role and name, neither can a screen reader.

| File | What it covers |
|---|---|
| `TodoService.test.k` | Adding, trimming, ordering and unique ids; empty, whitespace-only, at-limit and over-limit titles; renaming, including a blank rename and a removed todo; counts; removing; clearing completed (and that it doesn't write when nothing is done); filters; saving every change; restoring; unreadable data; blocked storage; a full quota; recovery after a failure; dismissing a problem. It pins the storage key name, because renaming it would lose users' data. |
| `AppearanceService.test.k` | The default scheme, choosing and saving, restoring, unreadable and blocked storage, a failed save. |
| `Panel.test.k` | The default heading level, setting it, placing the optional slots, and leaving them out. |
| `TodoList.test.k` | The scoped slot draws each todo in order, and the empty slot replaces the list. |
| `TodoRow.test.k` | Checking and unchecking, an edit starting from the current title, saving, cancelling, Save disabled for a blank draft, a too-long title, deleting. |
| `NewTodoField.test.k` | Adding with the button and with Enter, clearing the field, blank and too-long titles (shown as an alert, with the text kept), the error clearing after a successful add. |
| `FilterBar.test.k` | The pressed state, writing through `bind` to a host component's `@State`, writing through a custom `Binding`, the labelled landmark. |
| `Home.test.k` | The whole page with the real services: the empty state, adding with Enter, counts and the document title (including "1 item left"), every filter and its empty message, Clear completed, scoped queries for row buttons, a storage failure notice and dismissing it, the dark mode switch, and a saved scheme applied on first render. |

Every service test runs against `MemoryStorage` from `k.test`, which can also simulate the browser's failure modes (`failWrites(error: .quotaExceeded)`, `writeRaw(name:text:)`). Every test gets a fresh container, so no state leaks between tests.

## Choices worth noting

- **`@State` rather than `@Form` for the new-todo field.** The form has one field, and the title rules belong to `TodoService` because renaming applies them too. [15-forms-and-validation.md](../../docs/design/15-forms-and-validation.md) is the tool for multi-field forms whose struct is also an RPC request.
- **Components inject the service instead of emitting events.** `TodoRow` calls `todos.setDone(…)` itself, as the design's `AddToCart` example does, so the parent doesn't have to thread callbacks through `TodoList`.
- **`TodoList` doesn't know about `TodoRow`.** The scoped slot keeps the list reusable (the tests render it with plain `item`s), while the page decides what a row looks like.
- **Members of public types are marked `public` when other modules use them** (`TodoService.add`, `Todo.title`). [16-modules.md](../../docs/design/16-modules.md) makes "no modifier" mean visible within the module, and the weather example applies the same reading. Component props and slots carry no modifier, as in the design's own examples.

## Proposals this example relies on

These fill gaps in the design. Each is marked *Status: proposal* and is not yet accepted.

| Proposal | Used for |
|---|---|
| [k-toml.md](../../docs/design/proposals/k-toml.md) | The `k.toml` format |
| [app-structure.md](../../docs/design/proposals/app-structure.md) | The `app` declaration in `App.k`, and the `@Rendering(.client)` extension this example adds |
| [doc-comments.md](../../docs/design/proposals/doc-comments.md) | `///` documentation comments with `- Parameter`, `- Returns` and `- Throws` sections |
| [local-storage.md](../../docs/design/proposals/local-storage.md) | `LocalStorage`, `StorageKey<T>`, `StorageReadError`/`StorageWriteError`, `MemoryStorage` |
| [service-init.md](../../docs/design/proposals/service-init.md) | `init()` in `TodoService` and `AppearanceService` |
| [theme-variants.md](../../docs/design/proposals/theme-variants.md) | `Theme.use(…)`, `Theme.current` and the `.base` variant |
| [element-accessibility.md](../../docs/design/proposals/element-accessibility.md) | `checkbox(isChecked:)`, `.label` as an accessible name, `.pressed`, `.role` |
| [catch-binding.md](../../docs/design/proposals/catch-binding.md) | `catch let rejection` to keep a typed error as state |
| [testing-support.md](../../docs/design/proposals/testing-support.md) | `@Inject` of the test double, `fill`, and the query, state and scoping extension this example adds |
