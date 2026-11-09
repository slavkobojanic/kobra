import k.rpc.Channel
import k.rpc.RpcOnlyError
import k.rtc.IceServer
import k.rtc.Signal

/// ICE servers and one room's signalling, for anyone.
public protocol LabProtocol {
    /// The TURN servers of `K_TURN_URLS`, with fresh credentials.
    rpc iceServers() throws RpcOnlyError -> List<IceServer>

    /// Joins the room `room`'s signalling.
    rpc join(room: String) throws LabError -> Channel<Signal, Signal, RpcOnlyError>
}
