import k.rpc.Channel
import k.rpc.RpcOnlyError

/// `ChatEvent` is a channel's server message and an rpc's result: it keeps the stricter rule, so a removed case breaks.
public protocol ChatProtocol {
    /// The last event.
    rpc last() throws RpcOnlyError -> ChatEvent
    /// The room's channel.
    rpc room(id: Int) throws RpcOnlyError -> Channel<String, ChatEvent, RpcOnlyError>
}
