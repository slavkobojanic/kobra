import k.rpc.RpcOnlyError

/// The server side of `StatusProtocol`.
public service StatusApi implements StatusProtocol {
    rpc greeting() async throws RpcOnlyError -> String {
        return "Hello from the server"
    }
}
