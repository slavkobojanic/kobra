/// An email address, optionally with a display name
/// (`docs/design/31-email.md`). [`parse`] trims surrounding whitespace and
/// lowercases the domain part only.
@Native
public struct EmailAddress implements Equatable, Hashable, Codable, ConfigValue {
    /// A literal, validated and normalized at compile time.
    @Native
    @Literal
    init(_ literal: String) {}

    /// Parses runtime text: a bare address, such as `" ana@Example.COM "`
    /// (which becomes `ana@example.com`), or a display name and an address
    /// in angle brackets, such as `"Ana Doe <ana@Example.COM>"` (which
    /// becomes `Ana Doe <ana@example.com>`). A name with a comma or another
    /// special character is quoted: `"\"Doe, Ana\" <ana@example.com>"`.
    ///
    /// - Returns: the address, or `nil` if `text` isn't one or contains a
    ///   line break.
    static func parse(_ text: String) -> EmailAddress?

    /// The address part, `local@domain`.
    var address: String
    /// The display name, if any, unquoted: `Doe, Ana` for
    /// `"Doe, Ana" <ana@example.com>`.
    var displayName: String?
}
