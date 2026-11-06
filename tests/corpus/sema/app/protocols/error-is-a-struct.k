// An rpc's error type is an enum with the failure case, or `RpcOnlyError`.
struct CartError implements Error, Codable {
    let reason: String
}

protocol Store {
    rpc add() throws CartError -> Int
}
