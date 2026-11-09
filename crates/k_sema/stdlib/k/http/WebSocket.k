/// An open WebSocket, from [`WebSocketClient.connect`]
/// (`docs/design/44-channels.md`, "Raw WebSockets").
///
/// ```k,ignore
/// let socket = try await sockets.connect(request: WebSocketRequest(url: feed))
/// try await socket.send(.text("{\"subscribe\": \"BTC-USD\"}"))
///
/// for await message in socket.messages {
///     let tick: Tick = try message.json()
/// }
/// ```
@Native
public class WebSocket {
    /// `open`, `closing` or `closed(code:reason:)`; reactive.
    let state: WebSocketState
    /// The subprotocol the endpoint chose, or `nil`.
    let subprotocol: String?
    /// Every message the endpoint sends, in order; ends when the socket
    /// closes. Read once.
    let messages: Stream<WebSocketMessage>

    /// Sends `message`, waiting while more than 1 MB is still queued to go
    /// out.
    ///
    /// - Throws: [`WebSocketError.closed`] once the socket is closing or
    ///   closed, and [`WebSocketError.tooLarge`] once it closed because an
    ///   incoming message was over the request's `maxMessage`.
    func send(_ message: WebSocketMessage) async throws WebSocketError
    /// Closes the socket with `code` and `reason`; idempotent.
    func close(code: Int = 1000, reason: String = "")
}
