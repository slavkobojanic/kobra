/// Sets the JSON key of a [`Codable`] field when it differs from the field
/// name:
/// `@JsonKey("temperature_2m") let temperature: Double`
/// (`docs/design/32-http-client.md`). In the prelude: needs no import.
@Annotation
public struct JsonKey {
    /// Encodes and decodes the field under `key`.
    @Native
    init(_ key: String) {}
}
