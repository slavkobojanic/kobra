// Only a protocol's rpcs are served, so a `requires` clause on an rpc that no protocol declares
// guards nothing (K5507). There is no protocol rpc to move it to, and the rpc itself is K5508, with
// no fix: as a `func` the clause would be a syntax error.
import k.auth.Authenticated
import k.rpc.RpcOnlyError

service Lone {
    rpc hello() async throws RpcOnlyError -> Bool requires Authenticated {
        return true
    }
}
