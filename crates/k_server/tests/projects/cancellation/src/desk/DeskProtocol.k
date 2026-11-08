import k.auth.Authenticated
import k.rpc.RpcOnlyError

/// The rpcs the pages call.
public protocol DeskProtocol {
    /// Saves `text` and returns it.
    rpc save(text: String) throws RpcOnlyError -> String requires Authenticated
    /// Ends the session.
    rpc signOut() throws RpcOnlyError
    /// The name of `id`.
    rpc lookup(id: Int) throws RpcOnlyError -> String
}
