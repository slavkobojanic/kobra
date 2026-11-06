/// The standard `LocalStorage`, built on `window.localStorage`.
@ClientOnly
@Native
public service BrowserStorage implements LocalStorage {
    /// Reads from `window.localStorage`.
    func read<T>(key: StorageKey<T>) throws StorageReadError -> T? where T implements Codable
    /// Writes to `window.localStorage`.
    func write<T>(key: StorageKey<T>, value: T) throws StorageWriteError where T implements Codable
    /// Removes from `window.localStorage`.
    func remove<T>(key: StorageKey<T>)
}
