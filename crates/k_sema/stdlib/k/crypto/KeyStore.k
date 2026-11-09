import k.storage.StoragePersistence

/// Keeps secret and private keys in the browser across reloads without
/// writing them down as bytes (`docs/design/33-crypto.md`, "Keeping keys
/// in the browser"): [`BrowserKeyStore`] stores them in IndexedDB as
/// non-extractable Web Crypto keys. A client-only platform interface,
/// injected with no `use` rule; [`k.test.MemoryKeyStore`] replaces it in
/// tests. Server code holds keys in configuration, or wrapped in the
/// database.
///
/// A browser under storage pressure may delete the store, and a lost key
/// can't be recovered: keep a recovery path, such as a passphrase-wrapped
/// copy on the server, and ask for [`requestPersistence()`].
@Platform(side: .client)
public interface KeyStore {
    /// Reads the key kept under `name`.
    ///
    /// - Returns: the key, or `nil` when none is kept.
    /// - Throws: [`KeyStoreError.unavailable`] when IndexedDB is blocked.
    func get<K>(_ name: KeyName<K>) async throws KeyStoreError -> K?
    /// Keeps `key` under `name`, replacing what was there. An exportable
    /// key is stored non-exportable.
    ///
    /// - Throws: [`KeyStoreError.unavailable`] when IndexedDB is blocked.
    func put<K>(_ key: K, as name: KeyName<K>) async throws KeyStoreError
    /// Forgets the key kept under `name`, such as at sign-out.
    ///
    /// - Throws: [`KeyStoreError.unavailable`] when IndexedDB is blocked.
    func delete<K>(_ name: KeyName<K>) async throws KeyStoreError
    /// Asks the browser not to evict this origin's storage
    /// (`navigator.storage.persist()`). Chromium and WebKit answer
    /// silently; Firefox asks the person, so this needs a user's action.
    ///
    /// - Returns: what the browser granted.
    @UserAction
    func requestPersistence() async -> StoragePersistence
}
