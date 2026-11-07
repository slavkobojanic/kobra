/// A request an endpoint page's handler receives
/// (`docs/design/27-endpoint-pages.md`, "The incoming request"). Server-only.
@ServerOnly
@Native
public struct IncomingRequest {
    /// The request method ([`HttpMethod`]).
    var method: HttpMethod
    /// The absolute URL, built from `K_PUBLIC_ORIGIN`.
    var url: Url
    /// The query string's parameters.
    var query: Map<String, String>
    /// The headers, names lowercased ([`HttpHeaders`]).
    var headers: HttpHeaders
    /// The body exactly as received.
    var rawBody: Bytes
    /// The caller's address, honoring `K_TRUSTED_PROXIES`.
    var clientAddress: String

    /// Returns the body as UTF-8 text, or `nil` if it isn't valid UTF-8.
    func text() -> String?
    /// Decodes the body as JSON into a [`Codable`] type, given by the
    /// binding.
    ///
    /// - Returns: the decoded body.
    /// - Throws: [`DecodingError.malformed`] when the body isn't JSON, and
    ///   [`DecodingError.mismatch`] when it doesn't have the type's shape.
    func json<T>() throws DecodingError -> T where T implements Codable
}
