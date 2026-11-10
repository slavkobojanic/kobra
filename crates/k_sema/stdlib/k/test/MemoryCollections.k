import k.storage.Collection
import k.storage.CollectionIndex
import k.storage.Collections
import k.storage.StorageEstimate
import k.storage.StoragePersistence
import k.storage.StorageReadError
import k.storage.StorageWriteError

/// The default [`Collections`] in `k test`
/// (`docs/design/52-indexed-collections.md`, "Tests"): every collection in
/// memory, empty for every test. Values are kept in their JSON form and
/// decoded on every read, and ordered by the same keys as in the browser.
@TestDefault(replaces: Collections)
@Native
public service MemoryCollections implements Collections {
    /// Reads the value with `key`, or throws the error set by
    /// [`failReads(error:)`].
    func get<Key, Value>(_ key: Key, from collection: Collection<Key, Value>) async throws StorageReadError -> Value? where Key implements Codable, Value implements Codable
    /// Reads every value in key order, or throws the error set by
    /// [`failReads(error:)`].
    func all<Key, Value>(in collection: Collection<Key, Value>) async throws StorageReadError -> List<Value> where Key implements Codable, Value implements Codable
    /// Counts the values, or throws the error set by [`failReads(error:)`].
    func count<Key, Value>(in collection: Collection<Key, Value>) async throws StorageReadError -> Int where Key implements Codable, Value implements Codable
    /// Reads the values whose field equals `value`, or throws the error
    /// set by [`failReads(error:)`].
    func find<Field, Value>(_ value: Field, in index: CollectionIndex<Field, Value>) async throws StorageReadError -> List<Value> where Field implements Codable, Value implements Codable
    /// Reads a range of the index, or throws the error set by
    /// [`failReads(error:)`].
    func range<Field, Value>(in index: CollectionIndex<Field, Value>, from: Field? = nil, to: Field? = nil, reversed: Bool = false, limit: Int? = nil) async throws StorageReadError -> List<Value> where Field implements Codable, Value implements Codable
    /// Stores `value`, or throws the error set by [`failWrites(error:)`].
    func put<Key, Value>(_ value: Value, into collection: Collection<Key, Value>) async throws StorageWriteError where Key implements Codable, Value implements Codable
    /// Stores every value, or none and throws the error set by
    /// [`failWrites(error:)`].
    func putAll<Key, Value>(_ values: List<Value>, into collection: Collection<Key, Value>) async throws StorageWriteError where Key implements Codable, Value implements Codable
    /// Deletes the value with `key`, or throws the error set by
    /// [`failWrites(error:)`].
    func delete<Key, Value>(_ key: Key, from collection: Collection<Key, Value>) async throws StorageWriteError where Key implements Codable, Value implements Codable
    /// Deletes every value, or throws the error set by
    /// [`failWrites(error:)`].
    func clear<Key, Value>(_ collection: Collection<Key, Value>) async throws StorageWriteError where Key implements Codable, Value implements Codable
    /// Answers what [`setPersistence(_:)`] set, `.bestEffort` unless set.
    func persistence() async -> StoragePersistence
    /// Answers as [`persistence()`]; no prompt opens.
    func requestPersistence() async -> StoragePersistence
    /// Answers what [`setEstimate(_:)`] set, `nil` unless set.
    func estimate() async -> StorageEstimate?

    /// Stores the JSON `text` in the collection named `collection` exactly
    /// as given, as a value an older version of the app stored. Its key is
    /// read from the text's key field.
    func writeRaw(collection: String, text: String)
    /// Makes every following read throw `error`.
    func failReads(error: StorageReadError)
    /// Makes every following write throw `error`. Stored values are
    /// unchanged.
    func failWrites(error: StorageWriteError)
    /// Ends both simulations ([`failReads(error:)`] and
    /// [`failWrites(error:)`]).
    func stopFailing()
    /// Sets what [`persistence()`] and [`requestPersistence()`] answer.
    func setPersistence(_ value: StoragePersistence)
    /// Sets what [`estimate()`] answers.
    func setEstimate(_ estimate: StorageEstimate?)
}
