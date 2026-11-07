import k.rpc.RpcOnlyError

/// The server side of `SmokeProtocol`.
public service SmokeApi implements SmokeProtocol {
    rpc add(left: Int, right: Int) async throws RpcOnlyError -> Int {
        return left + right
    }
}
