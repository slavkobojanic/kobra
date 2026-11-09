# Local storage

Typed local storage is the **client-only `LocalStorage` interface** in `k.storage`, read and written
through **typed keys** whose names and value shapes are recorded in `storage.lock`. Its failures are
typed, because browser storage really does fail: it can be full, blocked, or hold data that no longer
decodes.

```k
// examples/todos/src/todo/TodoService.k
public service TodoService {
    @Inject let storage: LocalStorage
    // ...
    // Users' saved todos live under this key. Its name and the shape of
    // `Todo` are recorded in storage.lock, so `k check` fails if a change
    // would make every saved list unreadable.
    private let savedTodos: StorageKey<List<Todo>> = StorageKey(name: "todos")
    // ...
    @State var items: List<Todo> = []
    // ...
    @State var problem: StorageProblem? = nil
    // ...
    init() {
        restore()
    }
    // ...
    func restore() {
        do {
            items = try storage.read(key: savedTodos) ?? []
            problem = nil
        } catch .unreadable {
            items = []
            problem = .unreadable
        } catch .unavailable {
            items = []
            problem = .unavailable
        }
    }
    // ...
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
    // ...
}
```

## `LocalStorage`

```k
// crates/k_sema/stdlib/k/storage/LocalStorage.k
public interface LocalStorage {
    // ...
    func read<T>(key: StorageKey<T>) throws StorageReadError -> T? where T implements Codable
    // ...
    func write<T>(key: StorageKey<T>, value: T) throws StorageWriteError where T implements Codable
    // ...
    func remove<T>(key: StorageKey<T>)
}
```

- **Synchronous.** Browser local storage is synchronous, and the values it holds are small.
- **`read` returns `nil` when nothing is stored under the key.** Absence is normal, on a first visit for
  example, so it isn't an error.
- **Values are encoded as JSON through `Codable`.**
- The standard library ships one production implementation, `BrowserStorage`, so
  `@Inject let storage: LocalStorage` resolves to it with no `use` rule.

### Client-only

- **`LocalStorage` is client-only.** A service that injects it is client-only too.
- **The server never touches it, by construction.** In a server-rendered app, a client-only service
  exists during server rendering with its declared initial state, and its `init()` doesn't run; in the
  browser, `init()` runs after hydration. Calling `LocalStorage` from code that can run during server
  rendering is a compile error.
- **A server-rendered page that reads such a service is a warning,** because its first paint would show
  the initial state and then jump. The quick fix adds `@Rendering(.client)` to the app
  ([The app declaration](app.md#rendering-on-the-client)).

## Keys

```k
~import k.storage.StorageKey
~
~import k.storage.LocalStorage
~import k.storage.StorageReadError
~import k.storage.StorageWriteError
~
~struct Todo implements Codable {
~    let title: String
~}
~
~service Todos {
~    @Inject let storage: LocalStorage
~    @State var items: List<Todo> = []
private let savedTodos: StorageKey<List<Todo>> = StorageKey(name: "todos")

~
~    func reload() throws StorageReadError {
let saved = try storage.read(key: savedTodos)          // List<Todo>?
~    }
~
~    func persist() throws StorageWriteError {
try storage.write(key: savedTodos, value: items)        // value must be a List<Todo>
~    }
~}
```

- **`StorageKey<T>` pairs a name with the type stored under it,** so a read can't ask for a different type
  than the write stored. The type is given by the binding's annotation, because K has no explicit generic
  arguments in expressions.
- **The name is a string literal,** and each name is declared once in the app. Two keys with the same name
  are a `k check` error, so two services can't corrupt each other's data.
- **Keys are namespaced per app,** so two K apps on one origin don't collide. The namespace is derived
  from `k.toml`'s `name` when first recorded in `storage.lock`, and read from the lock afterwards, so
  renaming the project doesn't orphan every user's data. The browser's item is `namespace:name`
  (`todos:todos`).
- **Data stored before namespacing moves once.** An item an older build stored under the bare name is
  read when the namespaced one is missing, then moved under the namespace, so users keep their data.

### `storage.lock`

A key's name and its value's shape are part of the app's stored format, which outlives every deploy.
`storage.lock` records the namespace and, for each key, its name and its value type's shape:

- Adding an optional field to the value type is compatible.
- Removing or renaming a field, changing its type, adding a required field, or renaming the key is a
  `k check` error unless the key's declaration is marked `@Breaking("reason")`. Users' existing values
  would then read as `.unreadable`, which the service must handle.
- Test code isn't recorded, so a test may declare a key with an existing name to inspect what the service
  stored.
- The same file records each background job's name and payload shape, because queued jobs are stored
  data too ([Background jobs](jobs.md#stored-payloads)).

> **Planned.** The first compiler releases compare a committed `storage.lock`'s keys and job payloads with
> the code (`K8008` for a key, `K8003` for a job), but don't write the file. Keys are namespaced with the
> committed lock's `namespace`, else the project's name. Two keys with the same name are reported
> (`K2405`), and a name that isn't a string literal (`K2403`).

#### File format

`storage.lock` is TOML, generated by the compiler and committed. Nobody edits it by hand. This is the
lock of a to-do app with one key:

```toml
# Generated by the compiler. Do not edit by hand.
# Format: docs/design/36-local-storage.md

namespace = "todos"

[keys.todos]
declaration = "todo.TodoService.savedTodos"
type = "List<todo.Todo>"

[types."todo.Todo"]
fields = [
    { name = "id", type = "todo.TodoId" },
    { name = "title", type = "String" },
    { name = "isDone", type = "Bool" },
]

[types."todo.TodoId"]
fields = [
    { name = "value", type = "Uuid" },
]
```

- **`namespace`** is the storage namespace, derived from `k.toml`'s `name` when first recorded and read
  from here afterwards.
- **`[keys.<name>]`**, one table per `StorageKey`, keyed by the key's name (the string literal).
  `declaration` is the qualified name of the field that declares the key; `type` is the value type.
- **`[jobs.<name>]`**, one table per job, keyed by the job's name (from `@JobName`, or derived from the
  service's name when first recorded). `declaration` is the job service's qualified name; `payload` is
  the payload type, `"Void"` for a scheduled job:

  ```toml
  [jobs.order-confirmation]
  declaration = "orders.OrderConfirmationJob"
  payload = "orders.OrderConfirmation"
  ```

- **`declaration` follows the code.** When the declaration is renamed or moved, the compiler updates
  `declaration` and keeps the recorded name, so users' data and queued jobs stay reachable.
- **`[types."<qualified name>"]`** records the `Codable` shape of every app or package type reachable
  from a key's value or a job's payload:
  - a struct as `fields`, in declaration order, each with its `name` and `type`;
  - an enum without payloads as `cases`, a list of case names: `cases = ["light", "dark"]`;
  - an enum with payloads as `cases`, a list of tables `{ name = "…", fields = [ … ] }`, where a payload
    without a label has only a `type`.
- **Types are written as in K, with qualified names** (`List<todo.Todo>`, `String?`, `Map<String, Int>`).
  Standard-library types (`String`, `Int`, `Uuid`, `Date`, `List`, `Map`, `Optional`, and the rest) aren't
  expanded, because the standard library fixes their encoding.
- **Tables are sorted:** keys by name, then jobs by name, then types by qualified name, so the file
  doesn't depend on the order in which things were added.

## Errors

```k
// crates/k_sema/stdlib/k/storage/StorageReadError.k
/// Why a local-storage read ([`LocalStorage.read`]) failed.
public enum StorageReadError implements Error, Codable {
    /// Storage is blocked or disabled in this browser.
    case unavailable
    /// Something is stored, but it doesn't decode as the key's type.
    case unreadable
}
```

```k
// crates/k_sema/stdlib/k/storage/StorageWriteError.k
/// Why a local-storage write ([`LocalStorage.write`]) failed.
public enum StorageWriteError implements Error, Codable {
    /// Storage is blocked or disabled in this browser.
    case unavailable
    /// The origin's storage quota is used up.
    case quotaExceeded
}
```

- **`.unreadable` doesn't delete the stored value.** The app decides whether to overwrite it, keep it, or
  tell the user.
- `remove` never throws: removing from blocked storage, or removing a missing key, has nothing to report.

## Tests: `MemoryStorage`

**`k test` installs `MemoryStorage`** (from `k.test`) for `LocalStorage` by default, empty for every
test. A test reaches it with `@Inject let storage: MemoryStorage`:

| Member | Meaning |
|---|---|
| `writeRaw(name:text:)` | stores `text` exactly as given, bypassing encoding, to simulate stale or hand-edited data |
| `failReads(error: StorageReadError)` | makes every following `read` throw `error` |
| `failWrites(error: StorageWriteError)` | makes every following `write` throw `error`; the stored data is unchanged |
| `stopFailing()` | ends both simulations |

```k
// examples/todos/src/todo/TodoService.test.k
test TodoService {
    @Inject let storage: MemoryStorage
    @Inject let todos: TodoService
    // ...
    test "unreadable saved data starts an empty list and says so" {
        storage.writeRaw(name: "todos", text: "{{ this is not JSON")

        todos.restore()

        expect todos.items.isEmpty
        expect todos.problem == .unreadable
    }
    // ...
}
```

Note the `{{`: a single `{` would start an interpolation.

> **Open question.** Cross-tab change notifications, versioned keys with upgrade functions, and session
> storage and cookies in `k.storage` aren't designed yet.
