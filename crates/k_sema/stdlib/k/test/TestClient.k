import k.http.HttpRequest
import k.http.HttpResponse

/// Sends requests through the whole server stack in-process: routing,
/// guards, server rendering and endpoint pages
/// (`docs/design/21-testing.md`, "HTTP-level tests").
@Native
public service TestClient {
    /// Sends `request` through the server stack. Never throws.
    ///
    /// - Returns: the response the app produced, with any status.
    func send(request: HttpRequest) async -> HttpResponse
}
