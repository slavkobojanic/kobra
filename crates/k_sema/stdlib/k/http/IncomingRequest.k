/// A request an endpoint page's handler receives
/// (`docs/design/27-endpoint-pages.md`, "The incoming request"). Server-only.
@ServerOnly
@Native
public struct IncomingRequest {
    /// The method.
    var method: HttpMethod
    /// The absolute URL, built from `K_PUBLIC_ORIGIN`.
    var url: Url
    /// The query string.
    var query: Map<String, String>
    /// The headers, names lowercased.
    var headers: HttpHeaders
    /// The body exactly as received.
    var rawBody: Bytes
    /// The caller's address, honoring `K_TRUSTED_PROXIES`.
    var clientAddress: String

    /// The body as UTF-8, or `nil`.
    func text() -> String?
    /// The body decoded as JSON into a `Codable` type, given by the binding.
    func json<T>() throws DecodingError -> T where T implements Codable
}
