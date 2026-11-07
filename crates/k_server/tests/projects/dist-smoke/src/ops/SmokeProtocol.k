import k.rpc.RpcOnlyError

/// One rpc.
public protocol SmokeProtocol {
    rpc add(left: Int, right: Int) throws RpcOnlyError -> Int
}
