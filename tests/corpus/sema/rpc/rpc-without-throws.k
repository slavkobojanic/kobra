import k.rpc.RpcOnlyError

protocol Store {
    rpc products() -> List<Int>
    rpc count() throws RpcOnlyError -> Int
}
