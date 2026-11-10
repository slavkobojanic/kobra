/// The signature algorithms passkeys may use here
/// (`docs/design/51-passkeys.md`), in the browser's order of preference.
/// [`k.test.TestAuthenticator.useAlgorithm(_:)`] picks one.
public enum PasskeyAlgorithm implements Equatable, Hashable {
    /// ECDSA on P-256 with SHA-256 (COSE `-7`), what most devices use.
    case es256
    /// EdDSA on Ed25519 (COSE `-8`).
    case edDSA
    /// RSASSA-PKCS1-v1_5 with SHA-256 (COSE `-257`), for Windows Hello.
    case rs256
}
