/// An absolute URL. `Url("https://…")` takes a string literal validated at
/// compile time; runtime text goes through `Url.parse`
/// (`docs/design/32-http-client.md`).
@Native
public struct Url implements Equatable, Hashable, Codable, ConfigValue {
    /// A URL literal, validated at compile time.
    @Native
    @Literal
    init(_ literal: String) {}

    /// The path, such as `/v1/search`.
    var path: String
    /// The host name.
    var host: String

    /// Parses runtime text; `nil` if it isn't an absolute URL.
    static func parse(_ text: String) -> Url?

    /// The URL with `path` joined onto its path.
    func appending(path: String) -> Url
}
