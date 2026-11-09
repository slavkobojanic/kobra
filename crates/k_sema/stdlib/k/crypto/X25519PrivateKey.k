/// An X25519 key-agreement key (`docs/design/33-crypto.md`, "Key
/// agreement"): it agrees on a [`SharedSecret`] with another key and
/// opens [`Envelope`]s sealed to its public key. Secret: not
/// [`Codable`], [`Equatable`] or printable.
@Native
public struct X25519PrivateKey {
    /// The public key, to publish.
    var publicKey: X25519PublicKey

    /// Generates a key that can't be exported.
    ///
    /// - Returns: a fresh random key.
    static func generate() async -> X25519PrivateKey
    /// Generates a key that can be exported and wrapped.
    ///
    /// - Returns: the key, as an [`Exportable`].
    static func generateExportable() async -> Exportable<X25519PrivateKey>
    /// Imports a raw key (32 bytes), non-exportable.
    ///
    /// - Returns: the key, or `nil` for any other length.
    static func parse(raw: Secret<Bytes>) -> X25519PrivateKey?

    /// Agrees on a secret with the holder of `theirs`.
    ///
    /// - Returns: the shared secret, input to a key derivation only.
    func agree(with theirs: X25519PublicKey) async -> SharedSecret
    /// Opens an envelope sealed to this key's public key.
    ///
    /// - Parameter context: the context it was sealed with.
    /// - Returns: the value.
    /// - Throws: `.authenticationFailed` for another key, another context
    ///   or changed bytes; `.undecodable` when it isn't a `T`.
    func open<T>(_ envelope: Envelope<T>, context: Bytes = "".utf8) async throws OpenError -> T where T implements Codable
    /// Unwraps a key sealed to this key's public key.
    ///
    /// - Returns: the key, non-exportable.
    /// - Throws: `.authenticationFailed` for another key or changed bytes.
    func unwrap<K>(_ key: EnvelopedKey<K>) async throws OpenError -> K
}
