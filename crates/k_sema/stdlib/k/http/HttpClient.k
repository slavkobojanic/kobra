/// Calls external HTTP APIs (`docs/design/32-http-client.md`). A platform
/// interface usable on both sides and injectable only into services; its one
/// implementation, [`FetchHttpClient`], is injected with no `use` rule.
@Platform(side: .both)
public interface HttpClient {
    /// Sends `request` and returns the response, whatever its status.
    ///
    /// A non-2xx response is still a response: check
    /// [`HttpResponse.isSuccess`], or use [`fetchJson(request:)`].
    ///
    /// - Parameter request: the request to send.
    /// - Returns: the response, with any status.
    /// - Throws: [`HttpError.offline`] or [`HttpError.timeout`], never
    ///   [`HttpError.status`].
    func send(request: HttpRequest) async throws HttpError -> HttpResponse

    /// Sends the request, requires a 2xx status, and decodes the body as
    /// JSON into a [`Codable`] type, given by the binding.
    ///
    /// - Parameter request: the request to send.
    /// - Returns: the decoded body.
    /// - Throws: what [`send(request:)`] throws, [`HttpError.status`] for a
    ///   non-2xx response (with its body), and [`HttpError.decoding`] when
    ///   the body isn't the expected JSON.
    func fetchJson<T>(request: HttpRequest) async throws HttpError -> T where T implements Codable {
        let response = try await send(request: request)

        guard response.isSuccess else {
            throw HttpError.status(response.status, body: response.body)
        }

        return try response.json()
    }
}
