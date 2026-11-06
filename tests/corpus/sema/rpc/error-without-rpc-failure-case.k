import k.rpc.RpcFailure

enum CartError implements Error {
    case empty
}

enum OrderError implements Error {
    case soldOut
    case rpcFailure(RpcFailure)
}

protocol Store {
    rpc add() throws CartError -> Int
    rpc order() throws OrderError -> Int
}
