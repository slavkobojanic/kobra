/// The standard [`WebSocketClient`]: the browser's `WebSocket` in the
/// browser, and a Rust client on the server.
@Native
public service StandardWebSocketClient implements WebSocketClient {
    /// Opens a socket to the request's URL.
    ///
    /// - Parameter request: the request.
    /// - Returns: the open socket.
    /// - Throws: [`WebSocketError.offline`], [`WebSocketError.timeout`] or
    ///   [`WebSocketError.refused`].
    func connect(request: WebSocketRequest) async throws WebSocketError -> WebSocket
}
