/// An email address, optionally with a display name
/// (`docs/design/31-email.md`). [`parse`] trims surrounding whitespace and
/// lowercases the domain part only.
@Native
public struct EmailAddress implements Equatable, Hashable, Codable, ConfigValue {
    /// A literal, validated and normalized at compile time.
    @Native
    @Literal
    init(_ literal: String) {}

    /// Parses runtime text, such as `" ana@Example.COM "` (which becomes
    /// `ana@example.com`). The first compiler releases accept a bare
    /// address only: text with a display name, such as
    /// `"Ana <ana@example.com>"`, is `nil`.
    ///
    /// - Returns: the address, or `nil` if `text` isn't one or contains a
    ///   line break.
    static func parse(_ text: String) -> EmailAddress?

    /// The address part, `local@domain`.
    var address: String
    /// The display name, if any. Always `nil` in the first compiler
    /// releases, which don't parse display names.
    var displayName: String?
}
