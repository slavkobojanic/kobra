/// Opens WebSockets to third-party endpoints that speak their own protocol:
/// an exchange's price feed, a chat bridge, a local device
/// (`docs/design/44-channels.md`, "Raw WebSockets"). A platform interface
/// usable on both sides and injectable only into services; its one
/// implementation, [`StandardWebSocketClient`], is injected with no `use`
/// rule, and [`k.test.TestWebSocketClient`] replaces it in tests.
///
/// Nothing reconnects by itself, since the runtime can't know how to
/// resubscribe to someone else's protocol: a loop that waits
/// [`k.time.Backoff.next`] between attempts uses the channels' delays.
@Platform(side: .both)
public interface WebSocketClient {
    /// Opens a socket to the request's URL and waits for the handshake.
    ///
    /// The socket belongs to the run that opened it: when that run is
    /// cancelled (the page is left, the effect re-runs) the socket closes.
    ///
    /// - Parameter request: the URL, and optionally a subprotocol, headers
    ///   (server-only) and limits.
    /// - Returns: the open socket.
    /// - Throws: [`WebSocketError.offline`] when the endpoint can't be
    ///   reached, [`WebSocketError.timeout`] when the handshake takes longer
    ///   than the request's `timeout`, and [`WebSocketError.refused`] when
    ///   the endpoint answered without upgrading (on the server; a browser
    ///   doesn't say why, so there it is `offline`).
    func connect(request: WebSocketRequest) async throws WebSocketError -> WebSocket
}
