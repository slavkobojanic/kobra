/// Where a [`Channel`] is, for a banner (`docs/design/44-channels.md`,
/// "The client"). Read on the client, from `channel.state`.
public enum ChannelState implements Equatable {
    /// The first open, or a reopen after the socket came back.
    case connecting
    /// Open: messages flow.
    case open
    /// Dropped; the next attempt is due at `at`.
    case reconnecting(attempt: Int, at: Date)
    /// Closed for good, and why.
    case closed(ChannelClose)
}
