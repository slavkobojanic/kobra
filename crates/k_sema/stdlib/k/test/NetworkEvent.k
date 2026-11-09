/// A change to the in-process network of [`TestPeerConnections`].
public enum NetworkEvent implements Equatable {
    /// Every connection loses its path: `.reconnecting`, and `.closed(.failed)`
    /// 15 seconds of the [`TestClock`] later unless the network comes back.
    case networkLost
    /// Paths work again; an ICE restart reconnects.
    case networkRestored
    /// Every connection's ICE fails at once; the impolite peer restarts it.
    case iceFails
    /// Both peers of every connection offer at once, as when both add a
    /// track in the same moment; perfect negotiation resolves it.
    case glare
}
