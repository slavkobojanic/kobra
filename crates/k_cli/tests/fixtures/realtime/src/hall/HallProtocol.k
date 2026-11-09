import k.auth.Authenticated
import k.rpc.Channel
import k.rpc.RpcOnlyError

/// Halls with presence: who is in each one, live.
public protocol HallProtocol requires Authenticated {
    /// Joins a hall until the channel closes: who is there first, then
    /// every arrival, departure and status.
    rpc hall(name: String) throws RpcOnlyError -> Channel<HallCommand, HallEvent, RpcOnlyError>

    /// Who is in a hall now.
    rpc present(name: String) throws RpcOnlyError -> List<Member>

    /// Stands in a hall for the length of this one call only.
    rpc peek(name: String) throws RpcOnlyError -> List<Member>
}
