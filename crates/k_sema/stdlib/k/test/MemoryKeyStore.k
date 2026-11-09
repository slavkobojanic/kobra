import k.crypto.KeyName
import k.crypto.KeyStore
import k.crypto.KeyStoreError
import k.storage.StoragePersistence

/// The default [`KeyStore`] in `k test` (`docs/design/33-crypto.md`,
/// "Testing"): keys in memory, empty for every test.
@TestDefault(replaces: KeyStore)
@Native
public service MemoryKeyStore implements KeyStore {
    /// The names stored now, in alphabetical order.
    var names: List<String>
    /// What [`requestPersistence()`] answers: `.persistent` unless
    /// [`setPersistence(_:)`] said otherwise.
    var persistence: StoragePersistence

    /// Reads the key under `name`, or throws the error set by
    /// [`failAll(error:)`].
    func get<K>(_ name: KeyName<K>) async throws KeyStoreError -> K?
    /// Keeps `key` under `name`, or throws the error set by
    /// [`failAll(error:)`].
    func put<K>(_ key: K, as name: KeyName<K>) async throws KeyStoreError
    /// Forgets the key under `name`, or throws the error set by
    /// [`failAll(error:)`].
    func delete<K>(_ name: KeyName<K>) async throws KeyStoreError
    /// Answers [`persistence`].
    func requestPersistence() async -> StoragePersistence

    /// Makes every following `get`, `put` and `delete` throw `error`.
    func failAll(error: KeyStoreError)
    /// Ends [`failAll(error:)`].
    func stopFailing()
    /// Sets what [`requestPersistence()`] answers.
    func setPersistence(_ value: StoragePersistence)
}
