// An rpc's error enum has exactly one `rpcFailure(RpcFailure)` case.
import k.rpc.RpcFailure

enum CartError implements Error, Codable {
    case rpcFailure(RpcFailure)
    case otherFailure(RpcFailure)
}

protocol Store {
    rpc add() throws CartError -> Int
}
