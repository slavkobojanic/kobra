/// Why [`PeerConnections.connect(signalling:iceServers:policy:lifetime:)`]
/// failed.
public enum PeerError implements Error, Equatable {
    /// No `RTCPeerConnection` in this browser.
    case unsupported
    /// The channel ended before a peer joined.
    case signallingClosed
}
