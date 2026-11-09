import k.media.CaptureLifetime
import k.rpc.Channel
import k.rpc.RpcOnlyError
import k.rtc.IcePolicy
import k.rtc.IceServer
import k.rtc.PeerConnection
import k.rtc.PeerConnections
import k.rtc.PeerError
import k.rtc.Signal

/// The default [`PeerConnections`] in `k test`
/// (`docs/design/46-media-and-rtc.md`, "Loopback peers"): the real
/// negotiation over the real channel and relay, with an in-process media
/// engine, so nothing leaves the process. The other side of a call is a
/// [`TestCallPeer`] the test drives.
@TestDefault(replaces: PeerConnections)
@Native
public service TestPeerConnections implements PeerConnections {
    /// `true`.
    var isSupported: Bool

    /// See [`PeerConnections.connect(signalling:iceServers:policy:lifetime:)`].
    func connect(signalling: Channel<Signal, Signal, RpcOnlyError>, iceServers: List<IceServer>, policy: IcePolicy = .all, lifetime: CaptureLifetime = .owner) async throws PeerError -> PeerConnection
    /// A test-driven peer on `channel`, usually opened through a
    /// [`TestPeer`]'s client of the app's protocol, so the guards, the
    /// handler and the relay run as in production.
    ///
    /// - Throws: `.signallingClosed` when the channel ended before the
    ///   app's peer joined.
    func peer(joining channel: Channel<Signal, Signal, RpcOnlyError>) async throws PeerError -> TestCallPeer
    /// Changes the in-process network: `.networkLost`, `.networkRestored`,
    /// `.iceFails`, or `.glare` (both peers offer at once).
    func simulate(_ event: NetworkEvent)
}
