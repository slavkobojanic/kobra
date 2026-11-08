// Only a protocol's rpcs are served, so a `requires` clause on an rpc that no protocol declares
// guards nothing (K5507). There is no protocol rpc to move it to.
import k.auth.Authenticated
import k.rpc.RpcOnlyError

service Lone {
    rpc hello() async throws RpcOnlyError -> Bool requires Authenticated {
        return true
    }
}
