/// Which network paths a [`PeerConnection`] may use.
public enum IcePolicy implements Equatable {
    /// Direct paths, and a TURN relay when there is no other.
    case all
    /// Only through a TURN relay: for testing a deployment's TURN server,
    /// and for users who must not reveal their address to the peer.
    case relayOnly
}
