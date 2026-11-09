import k.rpc.RpcOnlyError

/// Speaks to the feed from the server, with its token.
public protocol RelayProtocol {
    /// Sends `text` to the feed and returns the subprotocol it chose.
    rpc relay(text: String) throws RpcOnlyError -> String
}
