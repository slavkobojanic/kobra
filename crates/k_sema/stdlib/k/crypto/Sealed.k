/// A value encrypted with an [`AesGcmKey`] (`docs/design/33-crypto.md`,
/// "Authenticated encryption"). The type parameter documents the content;
/// sealed values of different `T` don't convert into each other.
///
/// The wire form is a version byte `0x01`, the 12-byte nonce, the
/// ciphertext and the 16-byte tag; the text form, which `Codable` uses, is
/// `k1.` followed by that in base64url.
@Native
public struct Sealed<T> implements Codable, Equatable, Hashable where T implements Codable {
    /// The wire form, for storing outside Kobra.
    var bytes: Bytes

    /// Reads the wire form.
    ///
    /// - Returns: the sealed value, or `nil` if `bytes` isn't one.
    static func parse(bytes: Bytes) -> Sealed<T>?
    /// Reads the text form, `"{sealed}"`.
    ///
    /// - Returns: the sealed value, or `nil` if `text` isn't one.
    static func parse(_ text: String) -> Sealed<T>?
}
