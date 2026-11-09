import k.media.CaptureLifetime
import k.rpc.Channel
import k.rpc.RpcOnlyError

/// Peer-to-peer calls (`docs/design/46-media-and-rtc.md`, "Peer
/// connections"). A client-only platform interface:
/// [`BrowserPeerConnections`] is injected with no `use` rule, and
/// [`k.test.TestPeerConnections`] replaces it in tests.
@Platform(side: .client)
public interface PeerConnections {
    /// Whether this browser can make calls: `false` without
    /// `RTCPeerConnection`, and during server rendering.
    var isSupported: Bool

    /// Connects to the peer at the other end of `signalling`, the channel
    /// the app's protocol opened to a [`SignallingRelay`].
    ///
    /// Waits until the relay pairs this peer with another, then returns
    /// the connection in `.connecting`; media and data flow once it is
    /// `.connected`. The stdlib runs the whole negotiation.
    ///
    /// - Parameters:
    ///   - iceServers: the servers [`IceServers`] issued; empty on one
    ///     machine or one network.
    ///   - policy: `.relayOnly` sends everything through TURN.
    ///   - lifetime: `.owner` closes the connection, and the channel, when
    ///     the run that connected is cancelled.
    /// - Throws: [`PeerError.unsupported`], or `.signallingClosed` when the
    ///   channel ended before a peer joined.
    func connect(signalling: Channel<Signal, Signal, RpcOnlyError>, iceServers: List<IceServer>, policy: IcePolicy = .all, lifetime: CaptureLifetime = .owner) async throws PeerError -> PeerConnection
}
