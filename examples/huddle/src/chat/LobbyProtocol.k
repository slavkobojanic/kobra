import k.auth.Authenticated
import k.rpc.Channel
import k.rpc.RpcOnlyError

/// The lobby, for signed-in people only: every room with the caller's
/// unread count, kept up to date as messages are posted, deleted and read,
/// on any replica.
public protocol LobbyProtocol requires Authenticated {
    /// The rooms, then every change to them, until the caller leaves.
    rpc lobby() throws RpcOnlyError -> Channel<LobbyCommand, LobbyEvent, LobbyError>
}
