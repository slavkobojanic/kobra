import k.core.Url
import k.http.WebSocket
import k.http.WebSocketClient
import k.http.WebSocketError
import k.http.WebSocketRequest

/// The default [`WebSocketClient`] in `k test`
/// (`docs/design/44-channels.md`, "Testing"): it connects only to URLs a
/// test scripted with [`serve(url:with:)`], and fails the test, naming the
/// URL, on any other. One per test, shared by the test's browser and
/// server, so a test scripts what a server service connects to too.
@TestDefault(replaces: WebSocketClient)
@Native
public service TestWebSocketClient implements WebSocketClient {
    /// Every request connected or refused so far, oldest first.
    var requests: List<WebSocketRequest>

    /// Connects to the endpoint scripted for the request's URL, or throws
    /// what [`fail(url:error:)`] set; a URL with nothing scripted fails the
    /// test.
    func connect(request: WebSocketRequest) async throws WebSocketError -> WebSocket
    /// Scripts the endpoint at `url` (compared without its query): each
    /// connection to it talks to `server`.
    func serve(url: Url, with server: TestWebSocketServer)
    /// Makes connections to `url` throw `error`.
    func fail(url: Url, error: WebSocketError)
}
