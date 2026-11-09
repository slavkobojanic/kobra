// A browser can't send headers on a WebSocket handshake: setting them is server-only (K4031).
import k.core.Secret
import k.http.WebSocket
import k.http.WebSocketClient
import k.http.WebSocketError
import k.http.WebSocketRequest

/// Shared: it may run in the browser.
service Feed {
    @Inject let sockets: WebSocketClient

    func open() async throws WebSocketError -> WebSocket {
        let request = WebSocketRequest(url: Url("wss://feed.example/v1")).subprotocol("feed.v2").timeout(5.seconds).headers(["x-feed": "1"])
        return try await sockets.connect(request: request)
    }
}

/// Server-only: it bridges the feed for the browser, with its token.
@Scope(.server)
service Bridge {
    @Inject let sockets: WebSocketClient

    func open(token: Secret<String>) async throws WebSocketError -> WebSocket {
        let request = WebSocketRequest(url: Url("wss://feed.example/v1")).headers(["x-feed": "1"]).bearerToken(token)
        return try await sockets.connect(request: request)
    }
}
