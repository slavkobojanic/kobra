/// An immutable byte sequence with value semantics
/// (`docs/design/33-crypto.md`). `a + b` concatenates.
@Native
public struct Bytes implements Equatable, Hashable, Codable {
    /// The number of bytes.
    var count: Int
    /// Lowercase hex text.
    var hex: String
    /// Base64 text.
    var base64: String
    /// The bytes decoded as UTF-8; `nil` if they aren't valid UTF-8.
    var utf8Text: String?

    /// Parses hex text in either case; `nil` for odd lengths or other
    /// characters.
    static func parse(hex: String) -> Bytes?
    /// Parses Base64 text; `nil` if it isn't valid Base64.
    static func parse(base64: String) -> Bytes?
}
