import k.crypto.AesGcmKey
import k.crypto.EcdsaP256PrivateKey
import k.crypto.Ed25519PrivateKey
import k.crypto.X25519PrivateKey

/// Fixture keys by number (`docs/design/33-crypto.md`, "Testing"): the
/// same key for the same number in every test and every run, such as
/// `TestKeys.ed25519(1)`. They can be bound into configs with
/// `Secret(TestKeys.ed25519(1))`.
@Native
public class TestKeys {
    /// The AES-256-GCM key number `n`.
    ///
    /// - Returns: the key.
    static func aesGcm(_ n: Int) -> AesGcmKey
    /// The Ed25519 key number `n`.
    ///
    /// - Returns: the key.
    static func ed25519(_ n: Int) -> Ed25519PrivateKey
    /// The ECDSA P-256 key number `n`.
    ///
    /// - Returns: the key.
    static func ecdsaP256(_ n: Int) -> EcdsaP256PrivateKey
    /// The X25519 key number `n`.
    ///
    /// - Returns: the key.
    static func x25519(_ n: Int) -> X25519PrivateKey
}
