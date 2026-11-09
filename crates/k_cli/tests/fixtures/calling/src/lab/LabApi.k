import k.rpc.Channel
import k.rpc.RpcOnlyError
import k.rtc.IceServer
import k.rtc.IceServers
import k.rtc.Signal

/// The server side of `LabProtocol`.
public service LabApi implements LabProtocol {
    @Inject let rooms: LabRooms
    @Inject let ice: IceServers

    rpc iceServers() async throws RpcOnlyError -> List<IceServer> {
        return await ice.forClient(user: "lab", validFor: 1.hours)
    }

    rpc join(room: String) async throws LabError -> Channel<Signal, Signal, RpcOnlyError> {
        guard let peer = await rooms.relay.join(key: room) else {
            throw LabError.full
        }

        return Channel(sending: peer.signals) { signal => peer.forward(signal) }
    }
}
