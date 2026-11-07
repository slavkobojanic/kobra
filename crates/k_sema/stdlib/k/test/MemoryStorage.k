import k.storage.LocalStorage
import k.storage.StorageKey
import k.storage.StorageReadError
import k.storage.StorageWriteError

/// The default [`LocalStorage`] in `k test`: empty for every test, able to
/// simulate failures (`docs/design/36-local-storage.md`).
@TestDefault(replaces: LocalStorage)
@Native
public service MemoryStorage implements LocalStorage {
    /// Reads `key` from memory, or throws the error set by
    /// [`failReads(error:)`].
    func read<T>(key: StorageKey<T>) throws StorageReadError -> T? where T implements Codable
    /// Writes `value` under `key` in memory, or throws the error set by
    /// [`failWrites(error:)`].
    func write<T>(key: StorageKey<T>, value: T) throws StorageWriteError where T implements Codable
    /// Removes `key` from memory.
    func remove<T>(key: StorageKey<T>)

    /// Stores `text` under the key named `name` exactly as given, bypassing
    /// encoding, to test reading what an older version stored.
    func writeRaw(name: String, text: String)
    /// Makes every following [`read(key:)`] throw `error`.
    func failReads(error: StorageReadError)
    /// Makes every following [`write(key:value:)`] throw `error`.
    func failWrites(error: StorageWriteError)
    /// Ends both simulations ([`failReads(error:)`] and
    /// [`failWrites(error:)`]).
    func stopFailing()
}
