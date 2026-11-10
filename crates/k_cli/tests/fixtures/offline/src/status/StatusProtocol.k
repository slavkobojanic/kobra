import k.rpc.RpcOnlyError

/// A greeting from the server, to call while online and offline.
public protocol StatusProtocol {
    /// Says hello from the server.
    rpc greeting() throws RpcOnlyError -> String
}
