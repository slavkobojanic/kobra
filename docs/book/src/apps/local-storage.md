# Local storage

Browser storage comes in two typed shapes in `k.storage`: **local storage** for a few small values read
whole ([below](#localstorage)), and **indexed collections** in IndexedDB for many values looked up by key
and listed by a field ([Indexed collections](#indexed-collections)). Both record what they store in
`storage.lock` and fail with the same typed errors.

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
    private let savedTodos = StorageKey<List<Todo>>(name: "todos")
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
- **A server-rendered page that reads such a service is a warning (`K4013`),** because its first paint would show
  the initial state and then jump. When the read is in a component, the first quick fix renders just that
  component in the browser with `@Rendering(.client)` ([Components](components.md#rendering-in-the-browser-only));
  the other adds `@Rendering(.client)` to the app ([The app declaration](app.md#rendering-on-the-client)).

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
private let savedTodos = StorageKey<List<Todo>>(name: "todos")

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
  than the write stored. The type is written out, `StorageKey<List<Todo>>(name: "todos")`, or given by
  the binding's annotation.
- **The name is a string literal,** and each name is declared once in the app. Two keys with the same name
  are a `k check` error, so two services can't corrupt each other's data.
- **Keys are namespaced per app,** so two Kobra apps on one origin don't collide. The namespace is the one
  `storage.lock` records, so renaming the project doesn't orphan every user's data; without a lock it is
  `k.toml`'s `name`. The browser's item is `namespace:name` (`todos:todos`).
- **Data stored before namespacing moves once.** An item an older build stored under the bare name is
  read when the namespaced one is missing, then moved under the namespace, so users keep their data.

### `storage.lock`

A key's name and its value's shape are part of the app's stored format, which outlives every deploy.
`storage.lock` records the namespace and, for each key, its name and its value type's shape:

- Adding an optional field, or one with a default, to the value type is compatible.
- Removing or renaming a field, changing its type, adding a required field, or renaming the key is a
  `k check` error unless the key's declaration is marked `@Breaking("reason")`. Users' existing values
  would then read as `.unreadable`, which the service must handle.
- Test code isn't recorded, so a test may declare a key with an existing name to inspect what the service
  stored.
- The same file records each background job's name and payload shape, because queued jobs are stored
  data too ([Background jobs](jobs.md#stored-payloads)), and each [indexed collection](#indexed-collections).
- **`k build` writes it** when it differs from what the program implies, after `k check` accepted the
  change; `k check` only reads it (`K8008` for a key, `K8003` for a job, `K8009` for a collection, `K8010` for a [key store](crypto.md)'s key name). An
  entry the code no longer declares is dropped, and a lock that doesn't change isn't rewritten. Two keys
  with the same name are reported (`K2405`), and a name that isn't a string literal (`K2403`).

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
- **Types are written as in Kobra, with qualified names** (`List<todo.Todo>`, `String?`, `Map<String, Int>`).
  Standard-library types (`String`, `Int`, `Uuid`, `Date`, `List`, `Map`, `Optional`, and the rest) aren't
  expanded, because the standard library fixes their encoding.
- **`[collections.<name>]`**, one table per [collection](#indexed-collections): its `declaration`, its
  `key` field, its value `type` and its `indexes`, sorted:

  ```toml
  [collections.unsavedEdits]
  declaration = "notes.OfflineEdits.edits"
  key = "noteId"
  type = "notes.UnsavedEdit"
  indexes = ["savedAt"]
  ```

- **Tables are sorted:** keys by name, then collections, then jobs by name, then types by qualified name,
  so the file doesn't depend on the order in which things were added.

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

## Indexed collections

Many values, large values, or values looked up by something other than one name belong in **indexed
collections**: typed IndexedDB, through the client-only `Collections` interface. The notes example keeps
an edit that couldn't reach the server, so closing the browser doesn't lose it:

```k
// examples/notes/src/notes/OfflineEdits.k
public service OfflineEdits {
    @Inject let collections: Collections
    @Inject let clock: Clock

    // The collection's name, key and value shape are recorded in
    // storage.lock, and the browser's database is upgraded to its indexes.
    private let edits: Collection<Uuid, UnsavedEdit> = Collection(name: "unsavedEdits", key: "noteId")
    private let bySavedAt: CollectionIndex<Date, UnsavedEdit> = CollectionIndex(collection: "unsavedEdits", field: "savedAt")
    // ...
    func keep(note: Uuid, draft: NoteDraft) async -> Bool {
        let edit = UnsavedEdit(noteId: note, title: draft.title, body: draft.body, tags: draft.tags, savedAt: clock.now)

        do {
            try await collections.put(edit, into: edits)
            return true
        } catch .unavailable, .quotaExceeded {
            return false
        }
    }
    // ...
    func loadNewest() async {
        do {
            newest = try await collections.range(in: bySavedAt, reversed: true, limit: 5)
        } catch .unavailable, .unreadable {
            newest = []
        }
    }
}
```

### Collections and indexes

- **A `Collection<Key, Value>` names a set of `Codable` values and the field that keys them.**
  `Collection(name: "unsavedEdits", key: "noteId")`: the name is unique in the app, and `key` names a
  stored field of `Value` whose type is exactly `Key`. Each value has one key; putting a value with the
  same key replaces the old one.
- **A `CollectionIndex<Field, Value>` names a field to look values up and order them by.**
  `CollectionIndex(collection: "unsavedEdits", field: "savedAt")`: the field's type is `Field` or
  `Field?`, and a value whose field is `nil` isn't in that index. An index's name is its field.
- **Key and index fields are `String`, `Int`, `Uuid`, `Date` or an enum without payloads.** They order as
  IndexedDB orders keys: `Int` numerically, `Date` chronologically, text by UTF-16 code units, an enum by
  its case name.
- **Both are declared as a constant or a field, with string literals,** so the compiler can check them
  against the value type, set up the browser's database and record them in `storage.lock`. A field that
  doesn't exist, has another type or can't be a key is `K2411`, a second collection with the same name or
  a second index on the same field `K2410`, an index naming no collection of its value type `K2412`, and a
  computed name `K2403`. Test code may redeclare them to inspect what a service stored.

### `Collections`

| Member | Does |
|---|---|
| `get(_ key:from:)` | the value with `key`, or `nil` |
| `all(in:)` | every value, in key order |
| `count(in:)` | how many values there are |
| `find(_ value:in:)` | the values whose indexed field equals `value`, in key order |
| `range(in:from:to:reversed:limit:)` | the values whose indexed field is between `from` and `to` (both inclusive, either open when `nil`), in the field's order, backwards with `reversed: true`, at most `limit` |
| `put(_:into:)`, `putAll(_:into:)` | stores values, replacing those with the same key; `putAll` is all or nothing |
| `delete(_ key:from:)`, `clear(_:)` | deletes one value, or all of them |
| `persistence()`, `requestPersistence()` | whether the browser may evict the origin's storage, and asking it not to (`@UserAction`: Firefox asks the person) |
| `estimate()` | the origin's `StorageEstimate(usage:quota:)` in bytes, or `nil` |

- **Every call is `async` and one IndexedDB transaction.** IndexedDB commits a transaction as soon as it
  waits for nothing, so a read and then a write are two calls.
- **The errors are local storage's:** a read throws `StorageReadError` (`.unavailable`, or `.unreadable`
  when a stored value doesn't decode, which leaves it stored), a write `StorageWriteError`
  (`.unavailable` or `.quotaExceeded`).
- **Client-only, as `LocalStorage` is:** a service that injects `Collections` is client-only, and a
  component that shows what it read is usually `@Rendering(.client)`, as the notes example's
  `UnsavedEditsList` is.

### The browser's database

The compiler, not the app, owns IndexedDB's layout: one database per app, `k-collections:<namespace>`,
one object store per collection. The browser opens it, compares it with the collections and indexes the
build declares, and **upgrades it when one is missing, only ever adding:** a new index is filled from the
values already stored, and nothing is deleted, so a deploy can be rolled back. A tab still running the
previous build closes its connection when a newer tab upgrades and reopens it on its next call, so two
tabs of two builds share the database without blocking each other.

`storage.lock` holds a collection to its shape (`K8009`): adding an optional or defaulted field or an
index is compatible; removing, renaming or retyping a field, adding a required one or renaming the collection
needs `@Breaking("reason")` on its declaration; changing its key field is refused even then, since stored
values keep the key they were written with. Declare a new collection instead.

### Tests: `MemoryCollections`

**`k test` installs `MemoryCollections`** (from `k.test`) for `Collections`, empty for every test. It keeps
each value's JSON form, decodes it on every read and orders as the browser does:

| Member | Meaning |
|---|---|
| `writeRaw(collection:text:)` | stores the JSON `text` as a value an older version stored; its key is read from the text |
| `failReads(error:)`, `failWrites(error:)`, `stopFailing()` | as `MemoryStorage`'s |
| `setPersistence(_:)` | what `persistence()` and `requestPersistence()` answer, `.bestEffort` unless set |
| `setEstimate(_:)` | what `estimate()` answers, `nil` unless set |

```k
// examples/notes/src/notes/OfflineEdits.test.k
test OfflineEdits {
    @Inject let offline: OfflineEdits
    @Inject let collections: MemoryCollections
    @Inject let clock: TestClock
    // ...
    test "when this browser can't keep the edit either, it says so" {
        collections.failWrites(error: .quotaExceeded)

        let kept = await offline.keep(note: Uuid("1b4e28ba-2fa1-41d2-883f-0016d3cca427"), draft: draft(title: "Plan"))
        expect not kept
    }
    // ...
}
```

> **Open question.** Cross-tab change notifications, versioned keys with upgrade functions, session
> storage and cookies in `k.storage`, and, for collections, transactions over several calls or
> collections, unique and compound indexes, and live queries aren't designed yet.
