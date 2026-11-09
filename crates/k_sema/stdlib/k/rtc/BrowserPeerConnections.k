import k.media.CaptureLifetime
import k.rpc.Channel
import k.rpc.RpcOnlyError

/// The standard [`PeerConnections`], over the browser's
/// `RTCPeerConnection` with perfect negotiation.
@ClientOnly
@Native
public service BrowserPeerConnections implements PeerConnections {
    /// See [`PeerConnections.isSupported`].
    var isSupported: Bool

    /// See [`PeerConnections.connect(signalling:iceServers:policy:lifetime:)`].
    func connect(signalling: Channel<Signal, Signal, RpcOnlyError>, iceServers: List<IceServer>, policy: IcePolicy = .all, lifetime: CaptureLifetime = .owner) async throws PeerError -> PeerConnection
}
