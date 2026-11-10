import k.rpc.RpcOnlyError

/// The rpc the request leak test calls.
public protocol EchoProtocol {
    rpc echo(text: String, times: Int) throws RpcOnlyError -> List<String>
}
