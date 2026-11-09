import k.http.WebSocketMessage

/// A scripted WebSocket endpoint for [`TestWebSocketClient.serve(url:with:)`]:
/// the test plays the third party.
///
/// ```k,ignore
/// let feed = TestWebSocketServer()
/// sockets.serve(url: Url("wss://feed.example/v1"), with: feed)
/// feed.send(.text("{\"price\": 42}"))
/// expect feed.received == [.text("{\"subscribe\": \"BTC-USD\"}")]
/// ```
@Native
public class TestWebSocketServer {
    /// Creates an endpoint that accepts every connection and has sent and
    /// received nothing.
    @Native
    init() {}

    /// The messages the code under test sent, oldest first, across its
    /// connections.
    let received: List<WebSocketMessage>
    /// The number of connections open now.
    let connections: Int

    /// Sends `message` on every open connection, in order.
    func send(_ message: WebSocketMessage)
    /// Closes every open connection with `code` and `reason`.
    func close(code: Int = 1000, reason: String = "")
}
