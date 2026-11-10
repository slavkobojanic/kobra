# Todos: components and local state

`examples/todos` is the smallest complete app in the repository: a todo list that keeps everything in
the browser's local storage. It has no server code, no protocol and no database, so it shows the
browser half of K on its own: components, reactivity, styles, a theme, typed storage and component
tests.

```bash
k test examples/todos      # every *.test.k file
k dev examples/todos       # serve it on http://localhost:8080
k build examples/todos     # dist/public only: it has no server-only code
```

Every sample on this page is an excerpt of the example, checked against its source by the book's test,
so it can't drift from the code that ships.

## The layout

```text
todos/
├── k.toml
├── App.k                   @Rendering(.client) and app-wide metadata
├── storage.lock            the storage namespace, key names and value shapes
└── src/
    ├── todo/               the domain and its state: Todo, TodoId, Filter, TitleError,
    │                       StorageProblem, TodoService (+ tests)
    ├── ui/                 TodoTheme, and Panel, a reusable card with slots
    ├── testing/            test-only fixtures: Todo.sample(title:isDone:)
    └── home/               the page Home and its parts: NewTodoField, TodoList, TodoRow,
                            FilterBar, StorageNotice, ThemeToggle (+ tests)
```

Dependencies point one way: `home` imports `todo` and `ui`; nothing imports `home`. The components in
`home` are used only by `Home`, so they have no `public` and stay private to the module.

## The app turns server rendering off

```k
// examples/todos/App.k
@Rendering(.client)
@Offline
public app Todos {
    metadata {
        manifest(name: "Todos", shortName: "Todos")
        themeColor(.primary)
        locale(.english)
    }
}
```

Local storage exists only in the browser, so a server-rendered page would show an empty list and then
jump to the saved one. `@Rendering(.client)` turns server rendering off, and with no server-only code
`k build` emits static files only (see [The app declaration](../apps/app.md)).

`@Offline` makes the app open without a network: `k build` adds a service worker that keeps the build's
files and its one document in the browser ([Offline and installable apps](../apps/offline.md)).

There are no `use` rules: `LocalStorage` has exactly one production implementation, `BrowserStorage`,
so it is injected automatically, and `k test` installs `MemoryStorage` in its place.

## State lives in a service

`TodoService` holds the list and is the only code that changes it. Components read `todos.items` and
the derived counts, and call its methods:

```k
// examples/todos/src/todo/TodoService.k
public service TodoService {
    @Inject let storage: LocalStorage
    // ...
    @State var items: List<Todo> = []
    // ...
    @Derived let completedCount: Int = items.filter { todo => todo.isDone }.count
    // ...
    @Derived let remainingCount: Int = items.count - completedCount
    // ...
    func add(title: String) throws TitleError {
        let checkedTitle = try checked(title: title)

        items.append(Todo(id: TodoId(value: Uuid.random()), title: checkedTitle, isDone: false))
        save()
    }
    // ...
}
```

- `@State` in a service is shared by every component that injects it ([Services](../apps/services.md)).
- `@Derived` values recompute only when what they read changes ([Reactivity](../apps/reactivity.md)).
- A bad title is a typed error, `TitleError`, which the field that called `add` catches and shows.

Every change is written to local storage under a typed key, and a storage failure becomes a value the
page can show rather than an exception ([Local storage](../apps/local-storage.md)):

```k
// examples/todos/src/todo/TodoService.k
private func save() {
    do {
        try storage.write(key: savedTodos, value: items)
        problem = nil
    } catch .quotaExceeded {
        problem = .full
    } catch .unavailable {
        problem = .unavailable
    }
}
```

The `catch` is exhaustive over `StorageWriteError`; adding a case to that enum would make this a
compile error until it is handled.

## Data types

```k
// examples/todos/src/todo/Filter.k
public enum Filter implements Equatable, Hashable {
    // ...
    case all
    // ...
    case active
    // ...
    case completed
    // ...
    func includes(todo: Todo) -> Bool {
        return match self {
            case .all { true }
            case .active { not todo.isDone }
            case .completed { todo.isDone }
        }
    }
}
```

`match` is an expression and exhaustive, so a fourth filter can't be forgotten here.

## The page

`Home` owns one piece of state, the selected filter, and derives everything else:

```k
// examples/todos/src/home/Home.k
public page Home at "/" {
    @Inject let todos: TodoService
    @Inject let reminders: ReminderService
    @Inject let clock: Clock

    @State var filter: Filter = .all

    @Derived let visibleTodos: List<Todo> = todos.visible(filter: filter)

    @Derived let remainingLabel: String = if todos.remainingCount == 1 {
        "1 item left"
    } else {
        "{todos.remainingCount} items left"
    }
    // ...
    metadata {
        title(documentTitle)
        description("A small todo list that keeps everything in your browser.")
    }
```

Its view composes the module's components. `Panel` has a required trailing slot and optional chained
ones (`toolbar`, `actions`); `TodoList` has a scoped slot, `row`, that the page fills with a `TodoRow`:

```k
// examples/todos/src/home/Home.k
Panel(title: "Todos") {
    UpdateNotice()

    NewTodoField()

    if let problem = todos.problem {
        StorageNotice(problem: problem)
    }

    TodoList(todos: visibleTodos) {
        paragraph { emptyMessage }
            .class(.empty)
    }
    .row { todo =>
        TodoRow(todo: todo)
    }
}
.headingLevel(1)
.container(.board)
.toolbar {
    ThemeToggle()
}
.actions {
    text { remainingLabel }
        .class(.remaining)

    FilterBar(selection: bind filter)
    // ...
}
```

- `.headingLevel(1)` sets an optional prop; `.container(.board)` passes one of the page's classes to
  the panel's `@Stylable` hook ([Styles](../apps/styles.md#styling-child-components-stylable)).
- `FilterBar(selection: bind filter)` hands the bar a two-way binding to the page's state
  ([Components](../apps/components.md)).

## A row

`TodoRow` keeps its own editing state and switches between two layouts:

```k
// examples/todos/src/home/TodoRow.k
checkbox(isChecked: Binding(get: { todo.isDone }, set: markDone))
    .label(todo.title)

button {
    "Edit"

    VisuallyHidden { " {todo.title}" }
}
.class(.edit)
.on(click: startEditing)
```

`Binding(get:set:)` adapts a value the row doesn't own into a binding; `set: markDone` passes a
function by name. `VisuallyHidden` makes each row's buttons distinct for a screen reader ("Edit Buy
milk"), which is also how the tests find them.

The row's classes depend on state:

```k
// examples/todos/src/home/TodoRow.k
.class([.row, .done when todo.isDone, .editing when isEditing])
```

## Reminders

An open todo has "Remind me" where the browser can show notifications. The first click asks for
permission, and half an hour later the operating system shows the todo's title, also while the tab is
in the background:

```k
// examples/todos/src/home/Home.k
@Effect
func showReminders() async {
    guard reminders.hasPending else { return }

    while true {
        await clock.sleep(1.minute)
        await reminders.showDue()
    }
}
```

`ReminderService` keeps the waiting reminders and shows those that are due through `Notifications`
([Notifications](../apps/notifications.md)); the page checks once a minute, which is as often as
browsers run a background tab's timers anyway. The effect reads only `hasPending`, so it starts when a
reminder is set and stops once the last one is out. Reminders live as long as the tab: they aren't
saved with the todos. `k build` writes the service worker that shows them, `dist/public/k-sw.js`, next
to `index.html`.

## Working offline

Once the service worker has cached the build, todos opens with the network gone: every file comes from
the browser's cache, and the todos from local storage. A new deploy is fetched in the background, and
`UpdateNotice`, at the top of the panel, offers it:

```k
// examples/todos/src/home/UpdateNotice.k
component UpdateNotice {
    @Inject let updates: AppUpdates

    view {
        if updates.isAvailable {
            box {
                paragraph { "A new version of Todos is ready." }
                    .class(.message)

                button { "Reload" }
                    .on(click: updates.apply)
            }
            .role(.status)
            .class(.notice)
        }
    }
// ...
}
```

`AppUpdates.isAvailable` turns `true` once the new build's worker has taken over, and `apply()` reloads
into it. Nothing is lost: the todos are in local storage. `UpdateNotice.test.k` announces a build with
`TestAppUpdates`.

## The theme switch

```k
// examples/todos/src/home/ThemeToggle.k
component ThemeToggle {
    @Inject let theme: Theme

    func useDarkVariant(isOn: Bool) {
        theme.apply(if isOn { .dark } else { .base })
    }

    view {
        checkbox(isChecked: Binding(get: { theme.current == .dark }, set: useDarkVariant))
            .label("Dark mode")
    }
}
```

`theme.apply` swaps one class on the document root and re-renders nothing; the `Theme` service
remembers the choice itself ([Styles and themes](../apps/styles.md#switching-variants-at-runtime)).

## The tests

Tests sit next to the code as `Name.test.k` and find elements the way a screen reader does:

```k
// examples/todos/src/home/Home.test.k
test "added todos are listed and counted" {
    let screen = render(Home())
    let field = screen.textField(labeled: "New todo")

    field.fill("Buy milk")
    field.press(.enter)
    field.fill("Walk the dog")
    field.press(.enter)

    expect screen.items().count == 2
    expect screen.text("2 items left").isVisible
    expect screen.title == "Todos (2)"
}
```

No test needs a `use` rule: every test gets a fresh, empty `MemoryStorage` and a fresh `Theme`. A test
that wants to look inside storage, or simulate a full quota, injects `MemoryStorage` by its concrete
type ([Testing](../apps/testing.md)).
