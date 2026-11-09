import k.rpc.Channel
import k.rpc.RpcOnlyError

/// A channel turned into an rpc, and an rpc turned into a channel: both break deployed callers.
public protocol ChatProtocol {
    /// Was an rpc.
    rpc history(id: Int) throws RpcOnlyError -> Channel<String, String, RpcOnlyError>
    /// Was a channel.
    rpc room(id: Int) throws RpcOnlyError -> List<String>
}
