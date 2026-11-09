/// Why a [`PeerConnection`] closed.
public enum PeerClose implements Equatable {
    /// The app called `close()`, or its owner went away.
    case byApp
    /// The peer hung up.
    case byPeer
    /// The network didn't recover within 15 seconds.
    case failed
    /// The signalling channel closed for good.
    case signallingLost
}
