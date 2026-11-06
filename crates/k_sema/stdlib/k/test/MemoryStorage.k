import k.storage.LocalStorage
import k.storage.StorageKey
import k.storage.StorageReadError
import k.storage.StorageWriteError

/// The default `LocalStorage` in `k test`: empty for every test, able to
/// simulate failures (`docs/design/36-local-storage.md`).
@TestDefault(replaces: LocalStorage)
@Native
public service MemoryStorage implements LocalStorage {
    /// Reads from memory.
    func read<T>(key: StorageKey<T>) throws StorageReadError -> T? where T implements Codable
    /// Writes to memory.
    func write<T>(key: StorageKey<T>, value: T) throws StorageWriteError where T implements Codable
    /// Removes from memory.
    func remove<T>(key: StorageKey<T>)

    /// Stores `text` exactly as given, bypassing encoding.
    func writeRaw(name: String, text: String)
    /// Makes every following `read` throw `error`.
    func failReads(error: StorageReadError)
    /// Makes every following `write` throw `error`.
    func failWrites(error: StorageWriteError)
    /// Ends both simulations.
    func stopFailing()
}
