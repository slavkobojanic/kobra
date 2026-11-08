// The K5507 fix removes the imports that only the handler's clause used, except those of a guard it
// moves to a protocol in the same file: there `RateLimit` is still used once the fix has moved it, so
// its import stays.
import k.auth.RateLimit
import k.rpc.RpcOnlyError

protocol Notes {
    rpc add() throws RpcOnlyError
}

service NotesApi implements Notes {
    rpc add() async throws RpcOnlyError requires RateLimit(5.perMinute) {
    }
}
