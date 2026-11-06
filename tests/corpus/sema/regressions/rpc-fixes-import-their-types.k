// The machine-applicable K5501 and K5502 fixes name `RpcOnlyError` and
// `RpcFailure`; when the file doesn't import them, applying the fix alone gives
// K1202. Each fix must also insert the import.
enum CartError implements Error, Codable {
    case empty
}

protocol Store {
    rpc add() -> Int
    rpc remove() throws CartError -> Int
}
