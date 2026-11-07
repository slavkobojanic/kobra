// The protocol.
import k.rpc.RpcOnlyError

public protocol OrdersProtocol {
    rpc count() throws RpcOnlyError -> Int
}
