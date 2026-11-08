import k.auth.Authenticated
import k.auth.RateLimit
import k.rpc.RpcOnlyError

/// The handlers. The clause is on its own line; `Authenticated` is already the protocol's, so only
/// `RateLimit` moves (K5507), with the import the protocol's file then needs. Both imports of this file
/// are used only by the clause, so the fix removes them too.
public service NotesApi implements NotesProtocol {
    rpc count() async throws RpcOnlyError -> Int
        requires Authenticated, RateLimit(10.perMinute) {
        return 0
    }
}
