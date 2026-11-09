/// Where a [`WebSocket`] is.
public enum WebSocketState implements Equatable {
    /// Open: messages flow both ways.
    case open
    /// `close` was called; the endpoint hasn't confirmed yet.
    case closing
    /// Closed, with the close frame's code and reason (`1006` and an empty
    /// reason when the connection dropped without one).
    case closed(code: Int, reason: String)
}
