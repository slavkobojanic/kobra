import k.crypto.Uuid
import k.rpc.RpcOnlyError

/// Accounts over rpc.
public protocol AccountsProtocol {
    /// Registers an account.
    rpc signUp(id: Uuid, email: String, handle: String, name: String) throws SignUpError -> Int

    /// Changes a handle; a taken one is an error.
    rpc rename(id: Uuid, handle: String) throws SignUpError -> String

    /// Writes an audit line and then fails: nothing is kept.
    rpc failAfterWriting(id: Uuid) throws SignUpError -> Int

    /// An inner block that fails is rolled back alone.
    rpc nested(outer: Uuid, inner: Uuid) throws RpcOnlyError -> List<String>

    /// The audit messages.
    rpc audit() throws RpcOnlyError -> List<String>

    /// Whether the database answers.
    rpc healthy() throws RpcOnlyError -> Bool
}
