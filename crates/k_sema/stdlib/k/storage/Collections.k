/// Typed IndexedDB (`docs/design/52-indexed-collections.md`): reads and
/// writes the [`Collection`]s an app declares, and queries them by their
/// [`CollectionIndex`]es. A client-only platform interface;
/// [`BrowserCollections`] is injected with no `use` rule, and
/// [`k.test.MemoryCollections`] replaces it in tests. Every call is one
/// IndexedDB transaction.
@Platform(side: .client)
public interface Collections {
    /// Reads the value with `key`.
    ///
    /// - Returns: the value, or `nil` when none has the key.
    /// - Throws: [`StorageReadError.unavailable`] when IndexedDB is blocked,
    ///   and [`StorageReadError.unreadable`] when the stored value doesn't
    ///   decode as `Value`.
    func get<Key, Value>(_ key: Key, from collection: Collection<Key, Value>) async throws StorageReadError -> Value? where Key implements Codable, Value implements Codable
    /// Reads every value, in key order.
    ///
    /// - Throws: as [`get(_:from:)`], `.unreadable` when any stored value
    ///   doesn't decode.
    func all<Key, Value>(in collection: Collection<Key, Value>) async throws StorageReadError -> List<Value> where Key implements Codable, Value implements Codable
    /// Counts the values.
    ///
    /// - Throws: [`StorageReadError.unavailable`] when IndexedDB is blocked.
    func count<Key, Value>(in collection: Collection<Key, Value>) async throws StorageReadError -> Int where Key implements Codable, Value implements Codable
    /// Reads the values whose indexed field equals `value`, in key order.
    ///
    /// - Throws: as [`all(in:)`].
    func find<Field, Value>(_ value: Field, in index: CollectionIndex<Field, Value>) async throws StorageReadError -> List<Value> where Field implements Codable, Value implements Codable
    /// Reads the values whose indexed field is between `from` and `to`,
    /// both inclusive and either open when `nil`, in the field's order
    /// (backwards when `reversed`), at most `limit` of them.
    ///
    /// - Throws: as [`all(in:)`].
    func range<Field, Value>(in index: CollectionIndex<Field, Value>, from: Field? = nil, to: Field? = nil, reversed: Bool = false, limit: Int? = nil) async throws StorageReadError -> List<Value> where Field implements Codable, Value implements Codable
    /// Stores `value`, replacing the value with the same key.
    ///
    /// - Throws: [`StorageWriteError.unavailable`] when IndexedDB is
    ///   blocked, and [`StorageWriteError.quotaExceeded`] when the origin's
    ///   quota is used up.
    func put<Key, Value>(_ value: Value, into collection: Collection<Key, Value>) async throws StorageWriteError where Key implements Codable, Value implements Codable
    /// Stores every value in one transaction: all of them or none.
    ///
    /// - Throws: as [`put(_:into:)`].
    func putAll<Key, Value>(_ values: List<Value>, into collection: Collection<Key, Value>) async throws StorageWriteError where Key implements Codable, Value implements Codable
    /// Deletes the value with `key`; deleting a missing key succeeds.
    ///
    /// - Throws: [`StorageWriteError.unavailable`] when IndexedDB is blocked.
    func delete<Key, Value>(_ key: Key, from collection: Collection<Key, Value>) async throws StorageWriteError where Key implements Codable, Value implements Codable
    /// Deletes every value of `collection`.
    ///
    /// - Throws: [`StorageWriteError.unavailable`] when IndexedDB is blocked.
    func clear<Key, Value>(_ collection: Collection<Key, Value>) async throws StorageWriteError where Key implements Codable, Value implements Codable
    /// Whether the browser may evict this origin's storage, without asking
    /// (`navigator.storage.persisted()`).
    func persistence() async -> StoragePersistence
    /// Asks the browser not to evict this origin's storage
    /// (`navigator.storage.persist()`). Chromium and WebKit answer
    /// silently; Firefox asks the person, so this needs a user's action.
    ///
    /// - Returns: what the browser granted.
    @UserAction
    func requestPersistence() async -> StoragePersistence
    /// The origin's storage use and quota, or `nil` where the browser
    /// doesn't say (`navigator.storage.estimate()`).
    func estimate() async -> StorageEstimate?
}
