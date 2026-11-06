# Local storage

Typed local storage is the **client-only `LocalStorage` interface** in `k.storage`, read and written
through **typed keys** whose names and value shapes are recorded in `storage.lock`. Its failures are
typed, because browser storage really does fail: it can be full, blocked, or hold data that no longer
decodes.

```k
public service TodoService {
    @Inject let storage: LocalStorage

    // Users' saved todos live under this key.
    private let savedTodos: StorageKey<List<Todo>> = StorageKey(name: "todos")

    @State var items: List<Todo> = []
    @State var problem: StorageProblem? = nil

    init() {
        restore()
    }

    func restore() {
        do {
            items = try storage.read(key: savedTodos) ?? []
        } catch .unreadable {
            problem = .unreadable
        } catch .unavailable {
            problem = .unavailable
        }
    }

    func add(title: String) {
        items.append(Todo(id: Uuid.random(), title: title, isDone: false))
        save()
    }

    private func save() {
        do {
            try storage.write(key: savedTodos, value: items)
        } catch .quotaExceeded {
            problem = .full
        } catch .unavailable {
            problem = .unavailable
        }
    }
}
```

## `LocalStorage`

```k
public interface LocalStorage {
    func read<T>(key: StorageKey<T>) throws StorageReadError -> T? where T implements Codable

    func write<T>(key: StorageKey<T>, value: T) throws StorageWriteError where T implements Codable

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
private let savedTodos: StorageKey<List<Todo>> = StorageKey(name: "todos")

let saved = try storage.read(key: savedTodos)          // List<Todo>?
try storage.write(key: savedTodos, value: items)        // value must be a List<Todo>
```

- **`StorageKey<T>` pairs a name with the type stored under it,** so a read can't ask for a different type
  than the write stored. The type is given by the binding's annotation, because K has no explicit generic
  arguments in expressions.
- **The name is a string literal,** and each name is declared once in the app. Two keys with the same name
  are a `k check` error, so two services can't corrupt each other's data.
- **Keys are namespaced per app,** so two K apps on one origin don't collide. The namespace is derived
  from `k.toml`'s `name` when first recorded in `storage.lock`, and read from the lock afterwards, so
  renaming the project doesn't orphan every user's data.

### `storage.lock`

A key's name and its value's shape are part of the app's stored format, which outlives every deploy.
`storage.lock` records the namespace and, for each key, its name and its value type's shape:

- Adding an optional field to the value type is compatible.
- Removing or renaming a field, changing its type, adding a required field, or renaming the key is a
  `k check` error unless the key's declaration is marked `@Breaking("reason")`. Users' existing values
  would then read as `.unreadable`, which the service must handle.
- Test code isn't recorded, so a test may declare a key with an existing name to inspect what the service
  stored.

## Errors

```k
public enum StorageReadError implements Error {
    case unavailable        // storage is blocked or disabled in this browser
    case unreadable         // something is stored, but it doesn't decode as T
}

public enum StorageWriteError implements Error {
    case unavailable        // storage is blocked or disabled in this browser
    case quotaExceeded      // the origin's storage quota is used up
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
test TodoService {
    @Inject let storage: MemoryStorage
    @Inject let todos: TodoService

    test "unreadable saved data starts an empty list and says so" {
        storage.writeRaw(name: "todos", text: "{{ this is not JSON")

        todos.restore()

        expect todos.problem == .unreadable
        expect todos.items.isEmpty
    }
}
```

Note the `{{`: a single `{` would start an interpolation.

> **Open question.** Cross-tab change notifications, versioned keys with upgrade functions, and session
> storage and cookies in `k.storage` aren't designed yet.
