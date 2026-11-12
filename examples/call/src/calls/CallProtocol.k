import k.auth.Authenticated
import k.auth.RateLimit
import k.crypto.Uuid
import k.rpc.Channel
import k.rpc.RpcOnlyError
import k.rtc.IceServer
import k.rtc.Signal

/// Calls, for signed-in people only.
///
/// `join(room:)` is a **channel** of `Signal`s: the stdlib's `PeerConnection`
/// and `SignallingRelay` exchange descriptions and network candidates over
/// it, so the app never sees one. Media and chat go directly between the
/// two browsers, never through the server.
public protocol CallProtocol requires Authenticated {
    /// The STUN and TURN servers to connect through, with credentials that
    /// expire in an hour; none in development.
    rpc iceServers() throws RpcOnlyError -> List<IceServer> requires RateLimit(30.perMinute)

    /// A new room: an unguessable id, which its link carries.
    rpc createRoom() throws RpcOnlyError -> Uuid

    /// Joins a room's signalling until the caller leaves it.
    rpc join(room: Uuid) throws CallError -> Channel<Signal, Signal, RpcOnlyError>
}
