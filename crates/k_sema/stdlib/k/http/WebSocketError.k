/// Why a [`WebSocketClient`] call or a [`WebSocket.send`] failed
/// (`docs/design/44-channels.md`, "Raw WebSockets").
public enum WebSocketError implements Error, Codable {
    /// The endpoint couldn't be reached, or the browser refused the
    /// handshake without saying why.
    case offline
    /// The handshake took longer than the request's `timeout`.
    case timeout
    /// The endpoint answered the handshake without upgrading (on the
    /// server, which sees the answer).
    case refused(status: HttpStatus)
    /// An incoming message was over the request's `maxMessage`, so the
    /// socket closed (code 1009).
    case tooLarge
    /// The socket is closing or closed.
    case closed
}
