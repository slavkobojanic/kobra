// The rpcs of a protocol are its API: `private` on an rpc is K1301 at the modifier.
import k.rpc.RpcOnlyError

protocol Pings {
    rpc ping() throws RpcOnlyError -> Int
    private rpc secret() throws RpcOnlyError -> Int
}
