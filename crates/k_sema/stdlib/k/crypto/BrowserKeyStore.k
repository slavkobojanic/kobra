import k.storage.StoragePersistence

/// The standard [`KeyStore`]: the browser's IndexedDB, holding keys as
/// structured-cloned, non-extractable `CryptoKey`s, namespaced per app.
@ClientOnly
@Native
public service BrowserKeyStore implements KeyStore {
    /// Reads the key under `name`; see [`KeyStore.get(_:)`].
    func get<K>(_ name: KeyName<K>) async throws KeyStoreError -> K?
    /// Keeps `key` under `name`; see [`KeyStore.put(_:as:)`].
    func put<K>(_ key: K, as name: KeyName<K>) async throws KeyStoreError
    /// Forgets the key under `name`; see [`KeyStore.delete(_:)`].
    func delete<K>(_ name: KeyName<K>) async throws KeyStoreError
    /// Asks for persistent storage; see
    /// [`KeyStore.requestPersistence()`].
    @UserAction
    func requestPersistence() async -> StoragePersistence
}
