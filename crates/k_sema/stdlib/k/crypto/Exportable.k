/// A key that can leave the platform's key storage
/// (`docs/design/33-crypto.md`, "Exportable keys"), from
/// `generateExportable()`, `unwrapExportable(_:)` or `Exportable.parse`.
/// Whether a key can be exported is part of its type: a plain key has no
/// export members.
@Native
public struct Exportable<K> {
    /// The key itself, for every ordinary operation.
    var key: K

    /// Imports a raw key as exportable: `K` comes from the expected type,
    /// `let e: Exportable<AesGcmKey>? = Exportable.parse(raw: bytes)`.
    ///
    /// - Returns: the key, or `nil` if `raw` isn't one of `K`.
    static func parse(raw: Secret<Bytes>) -> Exportable<K>?
    /// Imports a private JWK as exportable.
    ///
    /// - Returns: the key, or `nil` if `jwk` isn't one of `K`.
    static func parse(jwk: Secret<String>) -> Exportable<K>?
    /// Imports a `PRIVATE KEY` PEM (PKCS #8) as exportable.
    ///
    /// - Returns: the key, or `nil` if `pem` isn't one of `K` (an
    ///   `AesGcmKey` has no PEM form).
    static func parse(pem: Secret<String>) -> Exportable<K>?

    /// The raw form: 32 bytes for AES, the Ed25519 seed and X25519, the
    /// scalar for P-256.
    ///
    /// - Returns: the bytes, as a [`Secret`].
    func exportRaw() -> Secret<Bytes>
    /// The private JWK, with `alg`, `use` and `key_ops` set for the type
    /// (`kty` `oct` for an `AesGcmKey`).
    ///
    /// - Returns: the JWK text, as a [`Secret`].
    func exportJwk() -> Secret<String>
    /// The `PRIVATE KEY` PEM (PKCS #8) of a private key. An `AesGcmKey`
    /// has no PEM form: exporting one panics.
    ///
    /// - Returns: the PEM text, as a [`Secret`].
    func exportPem() -> Secret<String>
}
