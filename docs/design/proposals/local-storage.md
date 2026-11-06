# Proposal: Typed Local Storage (`k.storage`)

> **Status: proposal.** Not yet accepted into the design. First used by `examples/todos`.

## Problem

[20-standard-library.md](../20-standard-library.md) lists "typed local storage" in `k.storage` but doesn't define its API, how values are typed and encoded, what can fail, or how tests replace it. Browser storage fails in practice: it can be full, blocked in private-browsing modes, or hold data that no longer decodes after an app update or a manual edit. An API that hides those failures leads to apps that lose data silently.

## Proposal

### `LocalStorage` is an interface

```k
public interface LocalStorage {
    func read<T>(key: StorageKey<T>) throws StorageReadError -> T? where T implements Codable

    func write<T>(key: StorageKey<T>, value: T) throws StorageWriteError where T implements Codable

    func remove<T>(key: StorageKey<T>)
}
```

- **Synchronous.** Browser local storage is synchronous, and the values it holds are small. Large or queryable data belongs in the typed IndexedDB collections, which will be async and need their own proposal.
- **`read` returns `nil` when nothing is stored under the key.** Absence is normal (a first visit), so it isn't an error.
- **Values are encoded as JSON through `Codable`** ([08-types.md](../08-types.md)), so any `Codable` struct, enum or collection can be stored and read back with its type.
- The stdlib ships one production implementation, `BrowserStorage`, built on `window.localStorage`. Since it is the only one, `@Inject let storage: LocalStorage` resolves to it with no `use` rule ([10-services.md](../10-services.md)).
- **Layering:** like `HttpClient` ([http-client.md](http-client.md)), `LocalStorage` is injectable only into services. Components read persisted state through the service that owns it, so encoding, versioning and error handling stay in one place.
- **Client-only:** `LocalStorage` reads the user's browser, which the server can't see. During server rendering, `read` throws `.unavailable` and `write` throws `.unavailable`. An app whose pages render from local storage should turn server rendering off with `@Rendering(.client)` ([app-structure.md](app-structure.md)), so the first paint already shows the stored data.

### Keys are typed

```k
private let savedTodos = StorageKey<List<Todo>>(name: "todos")

let saved = try storage.read(key: savedTodos)          // List<Todo>?
try storage.write(key: savedTodos, value: items)        // value must be a List<Todo>
```

- `StorageKey<T>` pairs a name with the type stored under it, so a read can't ask for a different type than the write stored. It is an ordinary struct with one field, `name: String`.
- **Keys are namespaced per app** by the stdlib (the package `name` from `k.toml` is used as a prefix), so two K apps on one origin don't collide.
- A key's name is part of the app's stored format. Renaming it loses users' data, which is why the example's tests spell the name out.

### Errors

Reads and writes fail in different ways, so each declares exactly what it throws ([09-errors.md](../09-errors.md)):

```k
public enum StorageReadError implements Error {
    case unavailable        // storage is blocked or disabled, or this is the server
    case unreadable         // something is stored, but it doesn't decode as T
}

public enum StorageWriteError implements Error {
    case unavailable        // storage is blocked or disabled, or this is the server
    case quotaExceeded      // the origin's storage quota is used up
}
```

- **`.unreadable` doesn't delete the stored value.** The app decides whether to overwrite it, keep it, or tell the user.
- `remove` never throws: removing from blocked storage, or removing a missing key, has nothing to report.

### Testing: `MemoryStorage`

`k.test` ships `MemoryStorage`, an in-memory `LocalStorage` that can also simulate the browser's failure modes. It follows the pattern of `TestClock` ([timers-and-clock.md](timers-and-clock.md)) and is reached with `@Inject` as in [testing-support.md](testing-support.md):

```k
import k.storage.LocalStorage
import k.test.MemoryStorage

test TodoService {
    use MemoryStorage for LocalStorage

    @Inject let storage: MemoryStorage
    @Inject let todos: TodoService

    test "unreadable saved data starts an empty list and says so" {
        storage.writeRaw(name: "todos", text: "{ this is not JSON")

        todos.restore()

        expect todos.problem == .unreadable
    }
}
```

| Member | Meaning |
|---|---|
| `writeRaw(name:text:)` | stores `text` exactly as given, bypassing encoding, to simulate stale or hand-edited data |
| `failReads(error: StorageReadError)` | makes every following `read` throw `error` |
| `failWrites(error: StorageWriteError)` | makes every following `write` throw `error`. The stored data is unchanged. |
| `stopFailing()` | ends both simulations |

Every test gets a fresh, empty `MemoryStorage`, because every test gets a fresh container.

## Why

- Typed keys plus `Codable` make "typed local storage" mean something: the compiler checks every read and write against one declaration.
- Separate error enums keep `catch` exact. A read never has to handle `.quotaExceeded`, and a write never has to handle `.unreadable`.
- An interface with a single production implementation gives tests a seam without configuration, as `HttpClient` and `Clock` do.

## Open questions

- **Cross-tab changes.** The browser fires a `storage` event when another tab writes. A `changes(key:) -> Stream<T?>` member would let a service re-read; it isn't needed until an example syncs tabs.
- **Migrations.** A `version` on `StorageKey`, with a migration function from the previous version's type, would turn `.unreadable` after an app update into an upgrade. Deferred until a real migration is needed.
- **Session storage and cookies** (also listed for `k.storage`) could share `StorageKey` and the error enums.
