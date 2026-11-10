// A protocol can't be generic (K5514 at its parameters): `rpc.lock` records
// every rpc's concrete types. Its rpcs aren't checked further.
import k.rpc.RpcOnlyError

protocol Store<T> {
    rpc get(id: Int) throws RpcOnlyError -> T
}
