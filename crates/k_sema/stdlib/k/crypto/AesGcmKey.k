/// An AES-256-GCM key (`docs/design/33-crypto.md`, "The key types"): it
/// seals and opens values ([`Sealed`]) and wraps other keys
/// ([`WrappedKey`]).
///
/// A key is secret: it isn't [`Codable`], [`Equatable`] or printable
/// (`"{key}"` is `AesGcmKey(•••)`), so no rpc, `@Load` field, job, entity
/// or log can carry it, and it can't be exported unless it was generated
/// with [`generateExportable()`]. In the browser it is a non-extractable
/// Web Crypto key.
@Native
public struct AesGcmKey {
    /// Generates a key that can't be exported.
    ///
    /// - Returns: a fresh random key.
    static func generate() async -> AesGcmKey
    /// Generates a key that can be exported and wrapped.
    ///
    /// - Returns: the key, as an [`Exportable`].
    static func generateExportable() async -> Exportable<AesGcmKey>
    /// Imports a raw key, non-exportable.
    ///
    /// - Parameter raw: 32 secret bytes.
    /// - Returns: the key, or `nil` for any other length.
    static func parse(raw: Secret<Bytes>) -> AesGcmKey?
    /// Derives a key from a key agreement with HKDF-SHA256 (RFC 5869).
    ///
    /// - Parameters:
    ///   - input: the [`SharedSecret`] of [`X25519PrivateKey.agree(with:)`].
    ///   - salt: a salt both sides know, possibly empty.
    ///   - info: the purpose, such as `"chat/v1"`, so two derivations from
    ///     one secret never collide.
    /// - Returns: the derived key, non-exportable.
    static func derive(from input: SharedSecret, salt: Bytes, info: String) async -> AesGcmKey
    /// Derives a key from secret bytes with HKDF-SHA256 (RFC 5869).
    ///
    /// - Returns: the derived key, non-exportable.
    static func derive(from input: Secret<Bytes>, salt: Bytes, info: String) async -> AesGcmKey
    /// Derives a key from a secret text (its UTF-8) with HKDF-SHA256.
    ///
    /// - Returns: the derived key, non-exportable.
    static func derive(from input: Secret<String>, salt: Bytes, info: String) async -> AesGcmKey
    /// Derives a per-purpose key from a configured key with HKDF-SHA256,
    /// whose raw bytes the runtime uses without exposing them.
    ///
    /// - Returns: the derived key, non-exportable.
    static func derive(from input: Secret<AesGcmKey>, salt: Bytes, info: String) async -> AesGcmKey
    /// Derives a key from a passphrase with PBKDF2-HMAC-SHA256 at the
    /// salt's work factor. Never derive from the sign-in password.
    ///
    /// - Parameters:
    ///   - passphrase: what the person typed.
    ///   - salt: a [`PassphraseSalt`], stored next to what the key protects.
    /// - Returns: the derived key, non-exportable.
    static func derive(fromPassphrase passphrase: String, salt: PassphraseSalt) async -> AesGcmKey

    /// Encrypts `value` with a fresh random nonce. `Bytes` are encrypted as
    /// they are, any other [`Codable`] value as its JSON.
    ///
    /// - Parameters:
    ///   - value: what to encrypt.
    ///   - context: authenticated but not encrypted, such as
    ///     `"note:{id}".utf8`; opening needs the same bytes.
    /// - Returns: the ciphertext.
    func seal<T>(_ value: T, context: Bytes = "".utf8) async -> Sealed<T> where T implements Codable
    /// Checks and decrypts `sealed`.
    ///
    /// - Parameters:
    ///   - sealed: a value sealed with this key.
    ///   - context: the context it was sealed with.
    /// - Returns: the value.
    /// - Throws: `.authenticationFailed` for a wrong key or context or
    ///   changed bytes; `.undecodable` when it is authentic but not a `T`.
    func open<T>(_ sealed: Sealed<T>, context: Bytes = "".utf8) async throws OpenError -> T where T implements Codable
    /// Seals an exportable key with this one, with the key type's name as
    /// the context.
    ///
    /// - Returns: the wrapped key, to store or send.
    func wrap<K>(_ key: Exportable<K>) async -> WrappedKey<K>
    /// Unwraps a key wrapped with this one.
    ///
    /// - Returns: the key, non-exportable.
    /// - Throws: `.authenticationFailed` for the wrong wrapping key, such
    ///   as one derived from a wrong passphrase.
    func unwrap<K>(_ wrapped: WrappedKey<K>) async throws OpenError -> K
    /// Unwraps a key so it can be wrapped again, for a new device.
    ///
    /// - Returns: the key, exportable.
    /// - Throws: `.authenticationFailed` for the wrong wrapping key.
    func unwrapExportable<K>(_ wrapped: WrappedKey<K>) async throws OpenError -> Exportable<K>
}
