/// Why a [`DataChannel`] message couldn't be sent.
public enum DataChannelError implements Error, Equatable {
    /// The channel or its connection closed.
    case closed
    /// The encoded message is over `limit` bytes (64 KiB).
    case tooLarge(limit: Int)
    /// The peer runs a build whose message type has another shape.
    case incompatible
}
