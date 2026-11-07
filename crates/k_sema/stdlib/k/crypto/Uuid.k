/// A UUID (`docs/design/33-crypto.md`, "Uuid"). Works as a `@Param` type: an
/// invalid id in a URL is a `404`.
@Native
public struct Uuid implements Equatable, Hashable, Codable, ConfigValue {
    /// A literal, validated at compile time: `Uuid("…")` with a string
    /// literal only.
    @Native
    @Literal
    init(_ literal: String) {}

    /// Parses canonical text (`8-4-4-4-12` hex digits) in either case.
    ///
    /// - Returns: the UUID, or `nil` if `text` isn't one.
    static func parse(_ text: String) -> Uuid?
    /// Returns a new version 4 UUID from a secure random source.
    static func random() -> Uuid
}
