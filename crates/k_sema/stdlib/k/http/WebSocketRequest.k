/// What [`WebSocketClient.connect`] opens (`docs/design/44-channels.md`,
/// "Raw WebSockets").
///
/// A `@Builder` struct like [`HttpRequest`]: the URL is a named argument and
/// every other field is a chained method,
/// `WebSocketRequest(url: Url("wss://feed.example/v1")).subprotocol("feed.v2")`.
///
/// **`headers`, `secretHeaders` and `bearerToken` are server-only:** a
/// browser can't send headers on a handshake, so setting them in code that
/// runs in the browser is a compile error (K4031), and a token in a header
/// never reaches client code.
@Builder
public struct WebSocketRequest {
    /// The `ws://` or `wss://` URL.
    let url: Url
    /// The subprotocol to ask for (`Sec-WebSocket-Protocol`); none when
    /// `nil`.
    let subprotocol: String? = nil
    /// Headers to send with the handshake. Server-only.
    let headers: Map<String, String> = [:]
    /// Headers whose values are secrets, redacted in logs. Server-only.
    let secretHeaders: Map<String, Secret<String>> = [:]
    /// A token sent as `Authorization: Bearer …`; none when `nil`.
    /// Server-only.
    let bearerToken: Secret<String>? = nil
    /// How long the handshake may take before [`WebSocketError.timeout`].
    let timeout: Duration = 30.seconds
    /// The largest incoming message, in bytes; a larger one closes the
    /// socket (code 1009) and the next `send` throws
    /// [`WebSocketError.tooLarge`].
    let maxMessage: Int = 1_000_000
}
