import k.crypto.AesGcmKey
import k.crypto.KeyName
import k.crypto.KeyStore
import k.crypto.KeyStoreError
import k.storage.StoragePersistence

/// Keeps one device key in the browser's key store.
public service KeyKeeper {
    @Inject let keys: KeyStore

    private static let deviceName = KeyName<AesGcmKey>(name: "device")

    /// The kept key, or a new one, kept for next time.
    func deviceKey() async throws KeyStoreError -> AesGcmKey {
        if let kept = try await keys.get(KeyKeeper.deviceName) {
            return kept
        }
        let fresh = await AesGcmKey.generate()
        try await keys.put(fresh, as: KeyKeeper.deviceName)
        return fresh
    }

    /// Forgets the key.
    func forget() async throws KeyStoreError {
        try await keys.delete(KeyKeeper.deviceName)
    }

    /// Asks the browser to keep the store.
    func persist() async -> StoragePersistence {
        return await keys.requestPersistence()
    }
}
