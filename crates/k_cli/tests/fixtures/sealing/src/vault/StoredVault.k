import k.crypto.AesGcmKey
import k.crypto.PassphraseSalt
import k.crypto.WrappedKey

/// A vault as the server stores it: the salt and the wrapped key.
public struct StoredVault implements Codable, Equatable {
    let salt: PassphraseSalt
    let wrappedKey: WrappedKey<AesGcmKey>
}
