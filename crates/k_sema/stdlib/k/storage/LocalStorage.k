/// Typed browser local storage (`docs/design/36-local-storage.md`). A
/// client-only platform interface; [`BrowserStorage`] is injected with no
/// `use` rule, and [`k.test.MemoryStorage`] replaces it in tests.
@Platform(side: .client)
public interface LocalStorage {
    /// Reads the value stored under `key`.
    ///
    /// - Returns: the value, or `nil` if nothing is stored.
    /// - Throws: [`StorageReadError.unavailable`] when storage is blocked,
    ///   and [`StorageReadError.unreadable`] when what is stored doesn't
    ///   decode as the key's type.
    func read<T>(key: StorageKey<T>) throws StorageReadError -> T? where T implements Codable
    /// Stores `value` under `key`, replacing what was there.
    ///
    /// - Throws: [`StorageWriteError.unavailable`] when storage is blocked,
    ///   and [`StorageWriteError.quotaExceeded`] when the origin's quota is
    ///   used up.
    func write<T>(key: StorageKey<T>, value: T) throws StorageWriteError where T implements Codable
    /// Removes what is stored under `key`. Never throws.
    func remove<T>(key: StorageKey<T>)
}
