/// A UUID (`docs/design/33-crypto.md`, "Uuid"). Works as a `@Param` type: an
/// invalid id in a URL is a `404`.
@Native
public struct Uuid implements Equatable, Hashable, Codable, ConfigValue {
    /// A literal, validated at compile time.
    @Native
    @Literal
    init(_ literal: String) {}

    /// Parses canonical text in either case; `nil` if it isn't a UUID.
    static func parse(_ text: String) -> Uuid?
    /// A version 4 UUID from a secure random source.
    static func random() -> Uuid
}
