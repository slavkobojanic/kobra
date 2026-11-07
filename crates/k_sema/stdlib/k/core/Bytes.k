/// An immutable byte sequence with value semantics
/// (`docs/design/33-crypto.md`). `a + b` concatenates.
@Native
public struct Bytes implements Equatable, Hashable, Codable {
    /// The number of bytes.
    var count: Int
    /// The bytes as lowercase hex text.
    var hex: String
    /// The bytes as Base64 text.
    var base64: String
    /// The bytes decoded as UTF-8; `nil` if they aren't valid UTF-8.
    var utf8Text: String?

    /// Parses hex text in either case.
    ///
    /// - Returns: the bytes, or `nil` for an odd length or a character
    ///   that isn't a hex digit.
    static func parse(hex: String) -> Bytes?
    /// Parses Base64 text.
    ///
    /// - Returns: the bytes, or `nil` if `base64` isn't valid Base64.
    static func parse(base64: String) -> Bytes?
}
