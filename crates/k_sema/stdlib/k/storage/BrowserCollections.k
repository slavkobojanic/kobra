/// The standard [`Collections`]: the browser's IndexedDB, one database per
/// app (`k-collections:<namespace>`), upgraded to the app's collections
/// and indexes on first use (`docs/design/52-indexed-collections.md`,
/// "Versions and upgrades").
@ClientOnly
@Native
public service BrowserCollections implements Collections {
    /// See [`Collections.get(_:from:)`].
    func get<Key, Value>(_ key: Key, from collection: Collection<Key, Value>) async throws StorageReadError -> Value? where Key implements Codable, Value implements Codable
    /// See [`Collections.all(in:)`].
    func all<Key, Value>(in collection: Collection<Key, Value>) async throws StorageReadError -> List<Value> where Key implements Codable, Value implements Codable
    /// See [`Collections.count(in:)`].
    func count<Key, Value>(in collection: Collection<Key, Value>) async throws StorageReadError -> Int where Key implements Codable, Value implements Codable
    /// See [`Collections.find(_:in:)`].
    func find<Field, Value>(_ value: Field, in index: CollectionIndex<Field, Value>) async throws StorageReadError -> List<Value> where Field implements Codable, Value implements Codable
    /// See [`Collections.range(in:from:to:reversed:limit:)`].
    func range<Field, Value>(in index: CollectionIndex<Field, Value>, from: Field? = nil, to: Field? = nil, reversed: Bool = false, limit: Int? = nil) async throws StorageReadError -> List<Value> where Field implements Codable, Value implements Codable
    /// See [`Collections.put(_:into:)`].
    func put<Key, Value>(_ value: Value, into collection: Collection<Key, Value>) async throws StorageWriteError where Key implements Codable, Value implements Codable
    /// See [`Collections.putAll(_:into:)`].
    func putAll<Key, Value>(_ values: List<Value>, into collection: Collection<Key, Value>) async throws StorageWriteError where Key implements Codable, Value implements Codable
    /// See [`Collections.delete(_:from:)`].
    func delete<Key, Value>(_ key: Key, from collection: Collection<Key, Value>) async throws StorageWriteError where Key implements Codable, Value implements Codable
    /// See [`Collections.clear(_:)`].
    func clear<Key, Value>(_ collection: Collection<Key, Value>) async throws StorageWriteError where Key implements Codable, Value implements Codable
    /// See [`Collections.persistence()`].
    func persistence() async -> StoragePersistence
    /// See [`Collections.requestPersistence()`].
    @UserAction
    func requestPersistence() async -> StoragePersistence
    /// See [`Collections.estimate()`].
    func estimate() async -> StorageEstimate?
}
