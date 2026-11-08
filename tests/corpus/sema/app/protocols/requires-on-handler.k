// A `requires` clause on a handler is never checked: the server runs the guards its protocol declares
// (11-protocols-and-rpc.md), so it's K5507, with a fix that moves the clause to the protocol's rpc,
// after its signature or into the clause it already has. A guard the protocol already declares is
// simply removed. The protocol's own clauses are fine.
import k.auth.Authenticated
import k.auth.RateLimit
import k.rpc.RpcOnlyError

protocol Notes {
    rpc list() throws RpcOnlyError -> Int
    rpc add() throws RpcOnlyError requires RateLimit(5.perMinute)
    rpc count() throws RpcOnlyError -> Int requires Authenticated
}

service NotesApi implements Notes {
    rpc list() async throws RpcOnlyError -> Int requires Authenticated {
        return 0
    }

    rpc add() async throws RpcOnlyError requires Authenticated, RateLimit(5.perMinute) {
    }

    rpc count() async throws RpcOnlyError -> Int requires Authenticated {
        return 0
    }
}
