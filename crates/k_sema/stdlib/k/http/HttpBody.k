/// The body of an outgoing request or of a response. Each case sets
/// `Content-Type`.
public enum HttpBody {
    /// A `Codable` value encoded as JSON.
    case json(Codable)
    /// UTF-8 text.
    case text(String)
    /// `application/x-www-form-urlencoded` fields.
    case form(Map<String, String>)
    /// Raw bytes with an explicit content type.
    case bytes(Bytes, contentType: String)
}
