/// Calls external HTTP APIs (`docs/design/32-http-client.md`). A platform
/// interface usable on both sides and injectable only into services; its one
/// implementation, `FetchHttpClient`, is injected with no `use` rule.
@Platform(side: .both)
public interface HttpClient {
    /// Sends `request` and returns the response, whatever its status.
    /// Throws `.offline` or `.timeout`, never `.status`.
    func send(request: HttpRequest) async throws HttpError -> HttpResponse

    /// Sends the request, requires a 2xx status, and decodes the body as
    /// JSON.
    func fetchJson<T>(request: HttpRequest) async throws HttpError -> T where T implements Codable {
        let response = try await send(request: request)

        guard response.isSuccess else {
            throw HttpError.status(response.status, body: response.body)
        }

        return try response.json()
    }
}
