/// An email address, optionally with a display name
/// (`docs/design/31-email.md`). `parse` trims surrounding whitespace and
/// lowercases the domain part only.
@Native
public struct EmailAddress implements Equatable, Hashable, Codable, ConfigValue {
    /// A literal, validated and normalized at compile time.
    @Native
    @Literal
    init(_ literal: String) {}

    /// Parses runtime text; `nil` if it isn't an address or contains a line
    /// break.
    static func parse(_ text: String) -> EmailAddress?

    /// The address part, `local@domain`.
    var address: String
    /// The display name, if any.
    var displayName: String?
}
