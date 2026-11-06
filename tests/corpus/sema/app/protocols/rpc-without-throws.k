// Every rpc declares `throws`; the fix inserts `throws RpcOnlyError` before the return type.
import k.rpc.RpcOnlyError

protocol Store {
    rpc products() -> List<Int>
    rpc count() throws RpcOnlyError -> Int
}
