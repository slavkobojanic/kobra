import k.crypto.AesGcmKey
import k.crypto.PassphraseSalt
import k.crypto.WrappedKey

/// A vault as the server stores it: the salt and the vault key wrapped with
/// a key derived from the passphrase. The server can't read the key inside.
public struct StoredVault implements Codable, Equatable {
    let salt: PassphraseSalt
    let wrappedKey: WrappedKey<AesGcmKey>
}
