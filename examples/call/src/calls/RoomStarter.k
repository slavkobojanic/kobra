import k.crypto.Uuid
import k.rpc.RpcOnlyError

/// Starts calls, for the start page: pages can't call a protocol
/// themselves.
public service RoomStarter {
    @Inject let api: CallProtocol

    /// A new room's id.
    func create() async throws RpcOnlyError -> Uuid {
        return try await api.createRoom()
    }
}
