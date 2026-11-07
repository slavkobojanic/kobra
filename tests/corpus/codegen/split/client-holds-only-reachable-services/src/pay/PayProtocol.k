import k.rpc.RpcOnlyError

public protocol PayProtocol {
    rpc pay(amount: Int) throws RpcOnlyError -> String
}
