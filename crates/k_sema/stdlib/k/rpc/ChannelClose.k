/// Why a [`Channel`] closed for good (`docs/design/44-channels.md`, "The
/// client"). Crosses the wire in a `close` frame.
public enum ChannelClose implements Equatable, Codable {
    /// `close()`, or the owner that opened it was cancelled.
    case byClient
    /// The handler's sending stream ended.
    case byServer
    /// Sign-out, expiry or `endAll`; the router reacts as for any rpc.
    case sessionEnded
    /// A restart was refused: a guard, or the handler threw.
    case refused
}
