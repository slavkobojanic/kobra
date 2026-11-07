/// The body of an outgoing request or of a response. Each case sets
/// `Content-Type`.
public enum HttpBody {
    /// A [`Codable`] value encoded as JSON (`application/json`).
    case json(Codable)
    /// UTF-8 text (`text/plain`).
    case text(String)
    /// Form fields, encoded as `application/x-www-form-urlencoded`.
    case form(Map<String, String>)
    /// Raw bytes with an explicit `contentType`.
    case bytes(Bytes, contentType: String)
}
