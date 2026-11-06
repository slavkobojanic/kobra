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

    /// The string without leading and trailing whitespace.
    func trimmed() -> String
    /// The string in lower case.
    func lowercased() -> String
    /// The string in upper case.
    func uppercased() -> String
    /// `true` if `other` occurs in the string.
    func contains(_ other: String) -> Bool
    /// `true` if the string starts with `prefix`.
    func startsWith(_ prefix: String) -> Bool
    /// `true` if the string ends with `suffix`.
    func endsWith(_ suffix: String) -> Bool
    /// The parts between occurrences of `separator`; with a `limit`, at
    /// most that many parts (the last one holds the rest).
    func split(separator: String, limit: Int? = nil) -> List<String>
    /// The string with every occurrence of `target` replaced.
    func replace(_ target: String, with replacement: String) -> String
    /// The first `count` characters, or the whole string if it is shorter.
    func prefix(_ count: Int) -> String
    /// The string repeated `count` times.
    func repeated(times count: Int) -> String
    /// The string cut to at most `length` characters, `ending` included
    /// when it was cut (`"Hello world".truncated(length: 8, ending: "…")`).
    func truncated(length: Int, ending: String = "…") -> String
}
