import k.auth.Authenticated
import k.auth.RateLimit
import k.rpc.RpcOnlyError

/// The notes of the signed-in user.
public protocol NotesProtocol requires Authenticated {
    /// How many notes there are.
    rpc count() throws RpcOnlyError -> Int requires RateLimit(10.perMinute)

    /// The newest note's id.
    rpc newest() throws RpcOnlyError -> Int
}
