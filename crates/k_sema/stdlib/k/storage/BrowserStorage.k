/// The standard [`LocalStorage`], built on `window.localStorage`.
@ClientOnly
@Native
public service BrowserStorage implements LocalStorage {
    /// Reads `key` from `window.localStorage`; see
    /// [`LocalStorage.read(key:)`].
    func read<T>(key: StorageKey<T>) throws StorageReadError -> T? where T implements Codable
    /// Writes `value` under `key` to `window.localStorage`; see
    /// [`LocalStorage.write(key:value:)`].
    func write<T>(key: StorageKey<T>, value: T) throws StorageWriteError where T implements Codable
    /// Removes `key` from `window.localStorage`.
    func remove<T>(key: StorageKey<T>)
}
