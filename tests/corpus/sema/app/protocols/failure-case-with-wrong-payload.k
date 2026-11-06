// The `rpcFailure` case carries an `RpcFailure`, nothing else.
enum CartError implements Error, Codable {
    case rpcFailure(String)
}

protocol Store {
    rpc add() throws CartError -> Int
}
