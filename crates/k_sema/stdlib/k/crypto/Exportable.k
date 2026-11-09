/// A key that can leave the platform's key storage
/// (`docs/design/33-crypto.md`, "Exportable keys"), from
/// `generateExportable()` or `unwrapExportable(_:)`. Whether a key can be
/// exported is part of its type: a plain key has no export members.
@Native
public struct Exportable<K> {
    /// The key itself, for every ordinary operation.
    var key: K

    /// The raw form: 32 bytes for AES, the Ed25519 seed and X25519, the
    /// scalar for P-256.
    ///
    /// - Returns: the bytes, as a [`Secret`].
    func exportRaw() -> Secret<Bytes>
}
