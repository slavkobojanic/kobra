import k.rpc.RpcOnlyError

/// What the browser can ask the server to do with accounts. Each call that
/// succeeds changes the session, and the browser's `Session` follows it.
public protocol AuthProtocol {
    /// Makes an account and signs it in.
    ///
    /// - Throws: `SignUpError.emailTaken` when the address is taken.
    rpc signUp(credentials: Credentials) throws SignUpError

    /// Signs an account in.
    ///
    /// - Throws: `SignInError.invalidCredentials` when the email address and
    ///   password don't match an account.
    rpc signIn(credentials: Credentials) throws SignInError

    /// Ends the session.
    rpc signOut() throws RpcOnlyError
}
