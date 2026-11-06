// The contract, shared by both sides.
import k.rpc.RpcOnlyError

public protocol CartProtocol {
    rpc count() throws RpcOnlyError -> Int
}
