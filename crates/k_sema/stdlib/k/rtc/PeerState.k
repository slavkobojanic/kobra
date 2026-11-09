/// Where a [`PeerConnection`] is.
public enum PeerState implements Equatable {
    /// The peer left the room; a new one may join.
    case waitingForPeer
    /// Negotiating, before media and data flow.
    case connecting
    /// Media and data flow.
    case connected
    /// The network changed; ICE restarts through the same channel, and
    /// after 15 seconds without recovery the connection closes `.failed`.
    case reconnecting
    /// Over, for good.
    case closed(PeerClose)
}
