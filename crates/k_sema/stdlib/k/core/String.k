/// Text: an immutable sequence of Unicode scalar values.
///
/// String literals are single-line and interpolate with `{ }`:
/// `"Total: {total}"`. `+` concatenates.
@Native
public struct String implements Equatable, Hashable, Comparable, Codable, ConfigValue {
    /// The number of characters.
    var count: Int
    /// `true` if the string has no characters.
    var isEmpty: Bool
    /// `true` if the string has at least one character.
    var isNotEmpty: Bool
    /// The UTF-8 encoding of the string.
    var utf8: Bytes

    /// Returns the string without leading and trailing whitespace.
    func trimmed() -> String
    /// Returns the string in lower case.
    func lowercased() -> String
    /// Returns the string in upper case.
    func uppercased() -> String
    /// Returns `true` if `other` occurs in the string.
    func contains(_ other: String) -> Bool
    /// Returns `true` if the string starts with `prefix`.
    func startsWith(_ prefix: String) -> Bool
    /// Returns `true` if the string ends with `suffix`.
    func endsWith(_ suffix: String) -> Bool
    /// Splits the string at each occurrence of `separator`.
    ///
    /// - Parameters:
    ///   - separator: the text between parts, not included in them.
    ///   - limit: the most parts to return, the last one holding the rest;
    ///     no limit when `nil`.
    /// - Returns: the parts, in order.
    func split(separator: String, limit: Int? = nil) -> List<String>
    /// Returns the string with every occurrence of `target` replaced by
    /// `replacement`.
    func replace(_ target: String, with replacement: String) -> String
    /// Returns the first `count` characters, or the whole string if it is
    /// shorter.
    func prefix(_ count: Int) -> String
    /// Returns the string repeated `count` times.
    func repeated(times count: Int) -> String
    /// Cuts the string to at most `length` characters, `ending` included
    /// when it was cut (`"Hello world".truncated(length: 8, ending: "…")`).
    ///
    /// - Returns: the string itself when it is short enough, otherwise its
    ///   start followed by `ending`.
    func truncated(length: Int, ending: String = "…") -> String
}
