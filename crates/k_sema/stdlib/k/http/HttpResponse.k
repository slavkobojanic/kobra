/// An HTTP response: received from [`HttpClient`], or sent by an endpoint
/// page's handler (`docs/design/32-http-client.md`, "Responses").
///
/// It follows the builder rule: the status is a named argument and the
/// headers and body are chained, `HttpResponse(status: .ok).body(.text("ok"))`.
@Native
public struct HttpResponse {
    /// Creates a response with `status`, no headers and no body.
    @Native
    init(status: HttpStatus) {}

    /// Returns a `303 See Other` response to `page`, a page of the app.
    static func redirect(to page: Page) -> HttpResponse
    /// Returns a `303 See Other` response to an external `url`.
    static func redirect(url: Url) -> HttpResponse

    /// Returns the response with `headers` added. Setting `set-cookie` is
    /// a compile error for a literal name.
    func headers(_ headers: Map<String, String>) -> HttpResponse
    /// Returns the response with `body` ([`HttpBody`]), which also sets
    /// `Content-Type`.
    func body(_ body: HttpBody) -> HttpResponse

    /// The status ([`HttpStatus`]).
    var status: HttpStatus
    /// `true` for a 2xx status.
    var isSuccess: Bool
    /// The headers, names lowercased ([`HttpHeaders`]).
    var headers: HttpHeaders
    /// The body exactly as received.
    var bodyBytes: Bytes
    /// The body as UTF-8 text.
    var body: String

    /// Decodes the body as JSON into a [`Codable`] type, given by the
    /// binding.
    ///
    /// - Returns: the decoded body.
    /// - Throws: [`HttpError.decoding`] when the body isn't the expected
    ///   JSON.
    func json<T>() throws HttpError -> T where T implements Codable
}
