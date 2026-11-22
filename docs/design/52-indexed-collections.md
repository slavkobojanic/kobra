# Indexed Collections

## Decision

**Typed IndexedDB is a client-only `Collections` interface in `k.storage`, over collections and indexes the app declares as typed values.** A `Collection<Key, Value>` names a set of `Codable` values and the field that keys them; a `CollectionIndex<Field, Value>` names a field of those values to look them up and order them by. Both are declared like a `StorageKey` ([36](36-local-storage.md#keys)): a constant or a field with string-literal arguments, checked by the compiler and recorded in `storage.lock`. Every operation is `async`, runs in one IndexedDB transaction, and throws `k.storage`'s existing `StorageReadError` or `StorageWriteError`. This is the IndexedDB row of [45's inventory](45-web-platform.md#inventory), Tier 2, and the collections [36](36-local-storage.md) deferred.

```k
import k.crypto.Uuid
import k.storage.Collection
import k.storage.CollectionIndex
import k.storage.Collections

/// Edits that couldn't reach the server, kept in this browser until they do.
public service OfflineEdits {
    @Inject let collections: Collections

    private let edits: Collection<Uuid, UnsavedEdit> = Collection(name: "unsavedEdits", key: "noteId")
    private let bySavedAt: CollectionIndex<Date, UnsavedEdit> = CollectionIndex(collection: "unsavedEdits", field: "savedAt")

    func keep(_ edit: UnsavedEdit) async throws StorageWriteError {
        try await collections.put(edit, into: edits)
    }

    func edit(of note: Uuid) async throws StorageReadError -> UnsavedEdit? {
        return try await collections.get(note, from: edits)
    }

    func newest() async throws StorageReadError -> List<UnsavedEdit> {
        return try await collections.range(in: bySavedAt, reversed: true, limit: 5)
    }
}
```

Five rules hold everywhere:

- **The shape is the code's, the database's layout is the compiler's.** The app never names an object store, a key path, a version number or an upgrade handler. The compiler derives the stores and indexes from the declarations, and the browser upgrades its database to them on first use ([Versions and upgrades](#versions-and-upgrades)).
- **Upgrades only add.** A new collection adds a store, a new index adds an index and fills it from the values already stored. Nothing is ever deleted or rebuilt by an upgrade, so an older tab still open beside a newer one keeps working, and two tabs can never undo each other's upgrade.
- **What users' browsers hold is recorded.** Each collection's name, key field, indexes and value shape are in `storage.lock`, with the compatibility rules of keys and `rpc.lock`. An incompatible change is a `k check` error (`K8009`) unless the collection is marked `@Breaking("reason")`; changing the key field is an error even then.
- **Client-only.** `Collections` is a platform interface of the client side ([10](10-services.md#layering)), so it is injected only into services, which become client-only, and calling it from code that can run during server rendering is the existing compile error. `Collection` and `CollectionIndex` are plain values on both sides.
- **Tested without a browser.** `k test` installs `MemoryCollections` (`k.test`) for `Collections`: the same ordering, the same codec round-trip, and simulated failures.

## Declarations

```k
public struct Collection<Key, Value> where Key implements Codable, Value implements Codable {
    let name: String
    let key: String
}

public struct CollectionIndex<Field, Value> where Field implements Codable, Value implements Codable {
    let collection: String
    let field: String
}
```

Both are plain structs built with their memberwise initializers, so they cost nothing in the browser until a `Collections` call uses them.

- **The type arguments come from the binding,** as for `StorageKey`, or are written out (`Collection<Uuid, UnsavedEdit>(name: …)`, [RA-23](../compiler/grammar.md)).
- **Every argument is a string literal.** A computed `name:` or `collection:` is `K2403`, as for a `StorageKey`'s name; a computed `key:` or `field:` is `K2411`.
- **They are declared as a constant or a field,** where the compiler finds them. A `Collection(…)` or `CollectionIndex(…)` anywhere else in non-test code (a local `let`, an argument) is `K2411`: the browser's database and `storage.lock` would never hear of it.
- **`Value` is a struct, and `key:` names one of its stored fields** (by its Kobra name, not a `@JsonKey`) whose type is exactly `Key`. The key field isn't optional: every value has a key.
- **`field:` names a stored field of `Value` whose type is `Field` or `Field?`.** A value whose indexed field is `nil` isn't in that index (IndexedDB doesn't index a missing key), so it is found by `get` and `all` but not by `find` or `range`.
- **Key and index fields have a key type:** `String`, `Int`, `Uuid`, `Date`, or an enum without payloads. Any other type (a struct such as `NoteId`, `Bool`, `Double`, a list) is `K2411`, with the list. They are ordered as IndexedDB orders keys: numbers before text; `Int` numerically, `Date` chronologically (stored as epoch milliseconds), text by UTF-16 code units, an enum by its case name.
- **Names are unique in the app's non-test code.** A second collection with the same name, or a second index on the same collection and field, is `K2410` at the later one's literal (file path order), with a label at the earlier one. Tests may redeclare a collection or index to inspect what a service stored, as they may a storage key.
- **An index names a declared collection with the same value type.** Anything else is `K2412`.
- **An index's name is its field.** There is no separate index name: `CollectionIndex(collection: "unsavedEdits", field: "savedAt")` is the index `savedAt`.

## `Collections`

```k
@Platform(side: .client)
public interface Collections {
    func get<Key, Value>(_ key: Key, from collection: Collection<Key, Value>) async throws StorageReadError -> Value?
    func all<Key, Value>(in collection: Collection<Key, Value>) async throws StorageReadError -> List<Value>
    func count<Key, Value>(in collection: Collection<Key, Value>) async throws StorageReadError -> Int
    func find<Field, Value>(_ value: Field, in index: CollectionIndex<Field, Value>) async throws StorageReadError -> List<Value>
    func range<Field, Value>(in index: CollectionIndex<Field, Value>, from: Field? = nil, to: Field? = nil, reversed: Bool = false, limit: Int? = nil) async throws StorageReadError -> List<Value>

    func put<Key, Value>(_ value: Value, into collection: Collection<Key, Value>) async throws StorageWriteError
    func putAll<Key, Value>(_ values: List<Value>, into collection: Collection<Key, Value>) async throws StorageWriteError
    func delete<Key, Value>(_ key: Key, from collection: Collection<Key, Value>) async throws StorageWriteError
    func clear<Key, Value>(_ collection: Collection<Key, Value>) async throws StorageWriteError

    func persistence() async -> StoragePersistence
    @UserAction func requestPersistence() async -> StoragePersistence
    func estimate() async -> StorageEstimate?
}
```

(every type parameter `implements Codable`; the stdlib file spells the `where` clauses out.)

- **`get` returns `nil` when nothing has the key;** absence is normal.
- **`put` inserts or replaces** the value with the same key. `putAll` writes every value in one transaction: all or none.
- **`all` is in key order; `find` returns the values whose field equals `value`, in key order; `range` returns those whose field is between `from` and `to`, both inclusive and either open when `nil`,** in the field's order (then key order), backwards with `reversed: true`, at most `limit` of them.
- **`delete` of a missing key and `clear` of an empty collection succeed:** there is nothing to report.
- **The errors are local storage's** ([36](36-local-storage.md#errors)): a read throws `.unavailable` (IndexedDB is blocked, disabled or failed to open) or `.unreadable` (a stored value doesn't decode as `Value`); a write throws `.unavailable` or `.quotaExceeded`. A list read that meets one value it can't decode throws `.unreadable` for the whole read, as `LocalStorage.read` does for its one value; the stored values are left alone, so the app can `clear` or overwrite them. The same two enums keep `catch` exact (a read never handles `.quotaExceeded`), and code that uses both kinds of storage handles one set of cases.
- **The stdlib's one production implementation is `BrowserCollections`,** so `@Inject let collections: Collections` needs no `use` rule.

### Quota and persistence

- **`requestPersistence()` asks the browser not to evict the origin's storage** (`navigator.storage.persist()`) and answers the existing `StoragePersistence` (`.persistent` or `.bestEffort`). It is `@UserAction`, because Firefox asks the person; Chromium and WebKit answer silently. It is the same origin-wide grant `KeyStore.requestPersistence()` asks for ([33](33-crypto.md)); both stay, so a service needs to inject only what it uses.
- **`persistence()` reads the grant without asking** (`navigator.storage.persisted()`), so an app can offer the request only when it would change something.
- **`estimate()` is `StorageEstimate(usage:quota:)`,** in bytes for the whole origin (`navigator.storage.estimate()`), or `nil` where the browser doesn't say. Browsers round and pad it, so it is for a "using 12 MB" line, not for deciding whether a write fits; a write that doesn't fit throws `.quotaExceeded`.
- Without the Storage API, `persistence()` and `requestPersistence()` answer `.bestEffort`.

## In the browser

- **One database per app,** `k-collections:<namespace>` with the storage namespace of `storage.lock` ([36](36-local-storage.md#keys)), so two Kobra apps on one origin don't share stores. One object store per collection, named after it.
- **A stored record is `{k, i, v}`:** `v` is the value's `Codable` JSON form ([28](28-rpc-wire-format.md)) as text, `k` the key field's IndexedDB key, `i` an object with the IndexedDB key of **every field of the value that has a key type**, indexed or not (absent for `nil`). Recording them all is what lets an older tab's writes appear in an index a newer build added: the older build doesn't know the index, but it knows the field. The store's key path is `k` and each index's is `i.<field>`. Keys are derived from the JSON form (a `Date` is parsed back to epoch milliseconds, so it orders chronologically, which its RFC 3339 text with optional milliseconds doesn't). Storing the JSON form rather than the runtime value keeps one encoding for every stored format, and makes "doesn't decode" the same check as for local storage and rpc.
- **Each operation is one transaction,** `readonly` for reads and `readwrite` for writes, opened and completed within the call. A transaction can't span an `await` of other work (IndexedDB commits it when its requests are done), so a read-modify-write is two calls; see [open questions](#open-questions).

### Versions and upgrades

- **The schema is compiled into the page:** for each collection its name, key field and indexes. The browser opens the database **without a version**, compares its stores and indexes with the schema and, when one is missing, closes it and reopens it at the next version, whose upgrade creates what is missing. No version number is recorded anywhere: the database's own version only ever counts upgrades.
- **A new index is filled from the values already stored:** the upgrade walks the store once and adds the new field's key to each record's `i`, so the index finds data written before it existed.
- **An upgrade never deletes.** A collection or index the code no longer declares stays in the database, so rolling a deploy back doesn't lose data and an older tab never finds its store gone. The key path of a store (`k`) and of an index (`i.<field>`) never change, whatever the declaration says, so there is nothing an upgrade would need to rebuild.
- **Tabs give way to each other's upgrades.** Every connection closes itself when another tab asks for a newer version (`versionchange`), so the upgrade isn't blocked, and the closed tab reopens the database on its next call. Its schema is older, so it is a subset of what the upgrade left: it opens without upgrading again. Two tabs of the same new build race for the same upgrade; the second finds it done.
- **A blocked upgrade waits.** A connection that doesn't close (another app's code on the same database name, which the namespace prevents) delays the open until it does; nothing fails.

### Server rendering

A service that injects `Collections` is client-only, so the server renders it with its declared initial state and its `init()` runs in the browser after hydration. A component that shows what it read is `@Rendering(.client)` ([47](47-client-rendered-components.md)), as for local storage.

## `storage.lock`

**`k build` now writes `storage.lock`,** after `k check` accepted the change, as it writes `rpc.lock`; `k check` only reads it. It records the namespace, every storage key ([36](36-local-storage.md#file-format)), every collection, every `KeyStore` key name ([33](33-crypto.md)), every job ([30](30-background-jobs.md#stored-payloads)) and the shapes of the app types they hold. The file format gains one table per collection:

```toml
[collections.unsavedEdits]
declaration = "notes.OfflineEdits.edits"
key = "noteId"
type = "notes.UnsavedEdit"
indexes = ["savedAt"]
```

- **`declaration`** is the qualified name of the constant or field; **`key`** the key field's Kobra name; **`type`** the value type, written as for keys; **`indexes`** the indexed fields, sorted. `[types."…"]` records the value type's shape as for keys.
- **Tables are sorted:** keys, then collections, then key names, then jobs, each by name, then types.
- **`KeyStore`'s key names are recorded too** (`[keyNames.<name>]` with `declaration` and the key `type`, [33](33-crypto.md#decided-while-implementing)), since the keys a browser keeps are stored data like values: a renamed `KeyName` is `K8010` unless it is marked `@Breaking("reason")`, and another key type is `K8010` even then, as a collection's key field is `K8009`.
- **The lock follows the code:** an entry the program no longer declares is dropped, a new one is added, `declaration` follows a rename, and an entry an `@Breaking` declaration changed records the new shape. A lock that doesn't change isn't rewritten. An app with no key, collection or job and no lock gets none.

### Compatibility (`K8009`)

A collection is matched with its entry by name, else by `declaration`:

- **Compatible:** adding an optional or defaulted field to the value type, a new enum case, a new index, removing an index.
- **`K8009`, unless the collection is marked `@Breaking("reason")`:** a change to the value's recorded shape that stored values wouldn't decode under (as for keys: a field removed, renamed or retyped, a required field added), or a renamed collection (the values stay under the old name). The values users' browsers hold would then read as `.unreadable`, or not at all, which the service must handle.
- **`K8009` always:** a different key field. Stored records keep the key they were written with, so the same value would have two keys. Declare a new collection under a new name instead; the old store is left in place.

## Tests: `MemoryCollections`

**`k test` installs `MemoryCollections`** (from `k.test`) for `Collections`, empty for every test. It stores each value's JSON form and decodes it on every read, and orders by the same keys as the browser, so a test sees what a browser would.

| Member | Meaning |
|---|---|
| `writeRaw(collection:text:)` | stores the JSON `text` as given, as a value an older version of the app stored; its key is read from the text's key field |
| `failReads(error: StorageReadError)` | every following read throws `error` |
| `failWrites(error: StorageWriteError)` | every following write throws `error`; stored values are unchanged |
| `stopFailing()` | ends both simulations |
| `setPersistence(_:)` | what `persistence()` and `requestPersistence()` answer; `.bestEffort` unless set |
| `setEstimate(_ estimate: StorageEstimate?)` | what `estimate()` answers; `nil` unless set |

```k
test OfflineEdits {
    @Inject let collections: MemoryCollections
    @Inject let edits: OfflineEdits

    test "an edit stored by an older version reads as unreadable" {
        collections.writeRaw(collection: "unsavedEdits", text: "{{\"noteId\": \"1b4e28ba-2fa1-41d2-883f-0016d3cca427\", \"title\": 3}")

        expect throws StorageReadError.unreadable {
            try await edits.newest()
        }
    }
}
```

**The stdlib's own tests:** the compiler checks and `K8009` in the sema corpus; the deferred chunk in the codegen corpus; `collections.test.js` against a scripted IndexedDB that checks the stores, key paths, versions, `versionchange` and blocked opens and the transactions each call makes; and real Chrome: values persist across a reload, an upgrade that adds an index finds the values stored before it, and two tabs of two builds share the database while one upgrades it.

## Server and client

| Declaration | Side |
|---|---|
| `Collections`, `BrowserCollections` | client-only |
| `Collection`, `CollectionIndex`, `StorageEstimate`, `StoragePersistence`, `StorageReadError`, `StorageWriteError` | both (values) |
| `MemoryCollections` | `k.test` |

**The browser code is a deferred chunk,** `collections.[hash].js`, shipped only to apps whose client injects `Collections` and preloaded only with the pages that reach it (codegen.md §16), measured against its own size budget. The shipped runtime chunk doesn't grow.

## Example

**`examples/notes` keeps edits that couldn't be saved.** When a save fails because the device is offline, the edit page keeps the person's draft in the `unsavedEdits` collection, keyed by the note; reopening the note, even after closing the browser, offers to save it or discard it, and the notes list shows the newest unsaved edits through the `savedAt` index. Notes was chosen over weather's recently viewed places: those are five small values read whole on every visit, which is what local storage is for, while drafts are many, up to 20,000 characters each, outlive the tab, and are looked up by note and listed by time, which is what an indexed collection is for.

## As built

Built tests first, as designed above, with these decisions and changes:

- **Declarations.** `k.storage` holds `Collections`, `BrowserCollections`, `Collection`, `CollectionIndex` and `StorageEstimate`; `k.test` holds `MemoryCollections`. `Collections` binds to `BrowserCollections` with no `use` rule (`client_containers` in `k_codegen`'s `app.rs`). The checks are `crates/k_sema/src/collections.rs`; the lock's reading, `K8009` and writing are `locks.rs` (`collections`, `implied`), exposed as `Analysis::collections()` and `Analysis::storage_lock()`.
- **`Collection` and `CollectionIndex` are plain structs, not `@Native` ones with `@Literal` initializers** as first drafted. A native initializer would have been the collections chunk's, so every chunk declaring a collection (the shared chunk, through a service's field) would have imported that chunk statically, and it would have shipped with every page. As plain structs they compile to object literals, and the chunk loads only with the pages that reach `Collections`. `K2403` for a computed name is now `collections.rs`' check, with the same code.
- **`i` holds every key-typed field, not just the indexed ones** (the [record](#in-the-browser)). The first version wrote only the indexed fields, and the scripted IndexedDB's two-tab test caught it: an older tab's write after a newer tab's upgrade was missing from the new index.
- **`MemoryCollections` has no `persistence` property:** it would have clashed with `Collections.persistence()`; a test reads the answer with `await collections.persistence()`.
- **Every `storage.lock` writer change applies to keys and jobs too:** `k build` now writes the lock for every app with a key, collection or job. Huddle gained a `storage.lock` (its `room-alert` job) and notes one (its `unsavedEdits` collection); `examples.rs`' `examples_commit_the_storage_lock_their_build_writes` keeps them current. A note shared by `K8003` and `K8008` said "recorded jobs" for keys too; it now says "recorded values" for keys and collections.
- **Real Chrome** (`crates/k_cli/tests/chrome.rs`, the `collections` fixture built with `k build` and served statically): `collections_persist_across_a_reload_in_chrome`, `collections_upgrade_adds_an_index_over_stored_values_in_chrome` (rebuilt with `Topics.k.v2`, which declares a `topic` index, into the same `dist`) and `collections_share_one_database_across_two_builds_in_two_tabs_in_chrome`.
- **Sizes.** The collections chunk is 1,956 B gzip compacted (budget 2 KB, `COLLECTIONS_SIZE_BUDGET`); notes ships it as 1,683 B. The shipped full client runtime didn't grow. **Notes' shipped-size budget went from 40,000 B to 44,000 B** (performance.md §11.7): it ships 41,949 B, 2,817 B more, with the chunk, `OfflineEdits`, `UnsavedEdit`'s codec and the two components.

## Why

- **Declared collections and indexes,** checked against the value type, make IndexedDB typed the way `StorageKey` makes local storage typed: a lookup can't ask for a field that isn't indexed or pass a value of the wrong type, and a typo is a compile error, not an empty result.
- **Compiler-owned layout and additive upgrades** remove IndexedDB's hardest part, versioning, from app code, and make the one real hazard, two tabs of two builds, safe by construction rather than by care.
- **Recording collections in `storage.lock`** treats users' browsers as the external system they are, as for keys and jobs.
- **Reusing `StorageReadError`, `StorageWriteError` and `StoragePersistence`** keeps `k.storage` one vocabulary.
- **One transaction per call** matches what IndexedDB can actually guarantee across `await`s and keeps the interface replaceable by an in-memory double.

## Alternatives rejected

- **Exposing IndexedDB's own model** (object stores, key paths, `onupgradeneeded`, version numbers): the hard parts would be the app's, untyped, and untestable without a browser.
- **Indexes listed on the collection** (`Collection(name:key:indexes: ["savedAt"])`) with lookups by field name: the field's type couldn't type the lookup's value, so every lookup would need its own compiler check. A `CollectionIndex<Field, Value>` is typed by the ordinary generic rules.
- **A version number on each collection, recorded in `storage.lock`:** it would make an older tab's open fail (`VersionError`) after any upgrade, and a forgotten bump would leave a new index missing. Comparing the database with the schema needs no number.
- **Storing the runtime value by structured clone:** fast, but Kobra's values (`Date`, enums, money) aren't plain objects with stable shapes, and decoding couldn't be checked the way `Codable` checks it.
- **Separate `CollectionReadError` and `CollectionWriteError`:** the cases are local storage's exactly.
- **A write transaction around a closure** (`collections.transaction { … }`): IndexedDB commits as soon as a closure awaits anything else, so the closure would have to be synchronous, which Kobra's async interfaces can't express yet.

## Open questions

- **Read-modify-write in one transaction:** an `update(_ key:in:change:)` taking a synchronous closure, once Kobra can say "this closure doesn't await".
- **Transactions over several collections**, for writes that must happen together.
- **Unique indexes** (`CollectionIndex(…, unique: true)`, a write that would duplicate throwing a new case).
- **Compound and multi-entry indexes** (`[tag, savedAt]`, an index over a `List<String>` field).
- **Migrating values on a `@Breaking` change** instead of `.unreadable`, as for versioned keys ([36](36-local-storage.md#open-questions)).
- **Live queries:** a stream of a range that updates when another tab writes, over `TabChannel`.
- **Cursors and paging** beyond `limit`, for collections too large to read whole.
