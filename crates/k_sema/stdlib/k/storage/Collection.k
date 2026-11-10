/// A set of `Codable` values in the browser's IndexedDB, keyed by one of
/// their fields (`docs/design/52-indexed-collections.md`):
/// `Collection<Uuid, Draft> = Collection(name: "drafts", key: "id")`.
/// Both arguments are string literals: `name` is declared once per app and
/// recorded in `storage.lock`, and `key` names a stored field of `Value`
/// whose type is exactly `Key` (`String`, `Int`, `Uuid`, `Date` or an enum
/// without payloads). Declared as a constant or a field, and read and
/// written through [`Collections`].
public struct Collection<Key, Value> where Key implements Codable, Value implements Codable {
    /// The collection's name, unique in the app.
    let name: String
    /// The name of the key field.
    let key: String
}
