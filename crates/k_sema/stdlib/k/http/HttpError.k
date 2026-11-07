/// Why an [`HttpClient`] call failed (`docs/design/32-http-client.md`,
/// "Errors"). Services map it to their own error enum.
public enum HttpError implements Error, Codable {
    /// The server couldn't be reached: a DNS failure, a refused
    /// connection or no network.
    case offline
    /// The request's [`HttpRequest.timeout`] elapsed.
    case timeout
    /// A non-2xx response, from [`HttpClient.fetchJson`], with the third
    /// party's body.
    case status(HttpStatus, body: String)
    /// The body wasn't the expected JSON.
    case decoding(message: String)
}
