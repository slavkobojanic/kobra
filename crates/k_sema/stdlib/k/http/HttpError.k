/// Why an `HttpClient` call failed (`docs/design/32-http-client.md`,
/// "Errors"). Services map it to their own error enum.
public enum HttpError implements Error, Codable {
    /// DNS failure, refused connection, no network.
    case offline
    /// The request's timeout elapsed.
    case timeout
    /// A non-2xx response, from `fetchJson`, with the third party's body.
    case status(HttpStatus, body: String)
    /// The body wasn't the expected JSON.
    case decoding(message: String)
}
