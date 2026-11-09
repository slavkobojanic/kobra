/// One message of the signalling between two peers
/// (`docs/design/46-media-and-rtc.md`, "Signalling over a channel"): the
/// `C` and `S` of the app's `Channel<Signal, Signal, RpcOnlyError>`.
///
/// Opaque to apps. Only [`PeerConnection`] and [`SignallingRelay`] create or
/// read one; an app passes them through its handler
/// (`Channel(sending: peer.signals) { signal => peer.forward(signal) }`)
/// and never builds a session description or parses one.
public enum Signal implements Codable, Equatable {
    /// The relay: another peer joined this one's room.
    case peerJoined
    /// The relay: the other peer left the room, or its channel closed.
    case peerLeft
    /// A peer's id, sent once per pairing; the two ids decide which peer is
    /// polite, so a peer that rejoins keeps its role.
    case hello(peer: String)
    /// A session description, an offer or an answer.
    case description(type: String, sdp: String)
    /// A network candidate.
    case candidate(candidate: String, mid: String?, line: Int?)
    /// The peer hung up.
    case bye
}
