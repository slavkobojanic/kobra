import k.rpc.RpcOnlyError

public protocol UpstreamProtocol {
    rpc describe(path: String, timeoutMs: Int) throws RpcOnlyError -> String
    rpc raw(path: String) throws RpcOnlyError -> String
    rpc unreachable() throws RpcOnlyError -> String
    rpc create() throws RpcOnlyError -> String
}
