import k.auth.Authenticated
import k.rpc.Channel
import k.rpc.RpcOnlyError

/// The lobby, as a channel, and an rpc that speaks in it from outside.
public protocol LobbyProtocol {
    /// Joins the lobby: every line so far, then each new one.
    rpc lobby() throws RpcOnlyError -> Channel<LobbyCommand, LobbyEvent, CommandError> requires Authenticated

    /// Adds a line from outside any channel.
    rpc announce(text: String) throws RpcOnlyError
}
