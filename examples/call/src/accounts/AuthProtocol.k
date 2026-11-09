import k.rpc.RpcOnlyError

/// Signing in and out.
public protocol AuthProtocol {
    /// Signs in under `name`. Call is a demo, so a name is all it asks
    /// for; `examples/notes` shows accounts with passwords.
    rpc signIn(name: String) throws SignInError

    /// Ends the session, and with it every channel opened under it.
    rpc signOut() throws RpcOnlyError
}
