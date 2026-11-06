/// An HTTP response: received from `HttpClient`, or sent by an endpoint
/// page's handler (`docs/design/32-http-client.md`, "Responses").
///
/// It follows the builder rule: the status is a named argument and the
/// headers and body are chained, `HttpResponse(status: .ok).body(.text("ok"))`.
@Native
public struct HttpResponse {
    /// A response with `status`, no headers and no body.
    @Native
    init(status: HttpStatus) {}

    /// `303 See Other` to a page of the app.
    static func redirect(to page: Page) -> HttpResponse
    /// `303 See Other` to an external URL.
    static func redirect(url: Url) -> HttpResponse

    /// The response with `headers` added. Setting `set-cookie` is a compile
    /// error for a literal name.
    func headers(_ headers: Map<String, String>) -> HttpResponse
    /// The response with `body`, which also sets `Content-Type`.
    func body(_ body: HttpBody) -> HttpResponse

    /// The status.
    var status: HttpStatus
    /// `true` for a 2xx status.
    var isSuccess: Bool
    /// The headers, names lowercased.
    var headers: HttpHeaders
    /// The body exactly as received.
    var bodyBytes: Bytes
    /// The body as UTF-8 text.
    var body: String

    /// The body decoded as JSON into a `Codable` type, given by the binding.
    func json<T>() throws HttpError -> T where T implements Codable
}
