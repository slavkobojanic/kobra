import k.auth.Authenticated
import k.rpc.RpcOnlyError

/// The notes of the signed-in user.
public protocol NotesProtocol requires Authenticated {
    /// How many notes there are.
    rpc count() throws RpcOnlyError -> Int
}
