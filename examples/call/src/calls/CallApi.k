import k.auth.Session
import k.crypto.Uuid
import k.rpc.Channel
import k.rpc.RpcOnlyError
import k.rtc.IceServer
import k.rtc.IceServers
import k.rtc.Signal

/// The server side of `CallProtocol`.
///
/// Whoever has a room's link may join it: the id is a random UUID, so the
/// link is the invitation. The protocol's `Authenticated` guard has run, so
/// `session.user` is never `nil` here.
public service CallApi implements CallProtocol {
    @Inject let rooms: CallRooms
    @Inject let ice: IceServers
    @Inject let session: Session

    rpc iceServers() async throws RpcOnlyError -> List<IceServer> {
        return await ice.forClient(user: session.user.id.name, validFor: 1.hours)
    }

    rpc createRoom() async throws RpcOnlyError -> Uuid {
        return Uuid.random()
    }

    rpc join(room: Uuid) async throws CallError -> Channel<Signal, Signal, RpcOnlyError> {
        // In the room until this channel closes: leaving the page, a
        // dropped connection that doesn't come back, or signing out.
        guard let peer = await rooms.relay.join(key: room) else {
            throw CallError.roomFull
        }

        return Channel(sending: peer.signals) { signal => peer.forward(signal) }
    }
}
