/// Typed browser local storage (`docs/design/36-local-storage.md`). A
/// client-only platform interface; `BrowserStorage` is injected with no
/// `use` rule, and `MemoryStorage` replaces it in tests.
@Platform(side: .client)
public interface LocalStorage {
    /// The value stored under `key`, or `nil` if nothing is stored.
    func read<T>(key: StorageKey<T>) throws StorageReadError -> T? where T implements Codable
    /// Stores `value` under `key`.
    func write<T>(key: StorageKey<T>, value: T) throws StorageWriteError where T implements Codable
    /// Removes what is stored under `key`. Never throws.
    func remove<T>(key: StorageKey<T>)
}
