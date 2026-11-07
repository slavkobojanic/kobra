/// An outgoing HTTP request (`docs/design/32-http-client.md`, "Requests").
///
/// A `@Builder` struct: the method and URL are named arguments and every
/// other field is a chained method,
/// `HttpRequest(method: .get, url: u).query(["q": text]).timeout(10.seconds)`.
@Builder
public struct HttpRequest {
    /// The request method ([`HttpMethod`]).
    let method: HttpMethod
    /// The absolute URL.
    let url: Url
    /// Query parameters, appended to [`url`].
    let query: Map<String, String> = [:]
    /// Headers to send.
    let headers: Map<String, String> = [:]
    /// Headers whose values are secrets, redacted in logs.
    let secretHeaders: Map<String, Secret<String>> = [:]
    /// A token sent as `Authorization: Bearer …`; none when `nil`.
    let bearerToken: Secret<String>? = nil
    /// The body ([`HttpBody`]), which also sets `Content-Type`; none when
    /// `nil`.
    let body: HttpBody? = nil
    /// How long to wait for a response before [`HttpError.timeout`].
    let timeout: Duration = 30.seconds
}
