/// A snapshot of a [`PeerConnection`]'s network, from `stats()`.
public struct PeerStats implements Equatable {
    /// The round-trip time of the path in use; `nil` before one is chosen.
    let roundTripTime: Duration?
    /// The bits per second the path can send, as the browser estimates
    /// it; `nil` when unknown.
    let outgoingBitrate: Int?
    /// The packets lost of what was received, every track together.
    let packetsLost: Int
    /// Whether the path in use goes through a TURN relay.
    let isRelayed: Bool
}
