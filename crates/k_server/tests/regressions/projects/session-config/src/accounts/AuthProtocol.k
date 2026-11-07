import k.auth.Authenticated
import k.auth.RateLimit
import k.auth.Role
import k.rpc.RpcOnlyError

/// Signing in and out, and rpcs behind each kind of guard.
public protocol AuthProtocol {
    /// Signs in `ada` (no roles) or `root` (admin) with the password `correct horse`.
    rpc signIn(credentials: Credentials) throws SignInError requires RateLimit(4.perMinute)

    /// Ends the current session.
    rpc signOut() throws RpcOnlyError

    /// Ends every session of the signed-in user.
    rpc signOutEverywhere() throws RpcOnlyError requires Authenticated

    /// Who is signed in, for anyone: `guest` without a session.
    rpc status() throws RpcOnlyError -> String

    /// The signed-in user's name.
    rpc whoami() throws RpcOnlyError -> String requires Authenticated

    /// Admins only.
    rpc adminReport() throws RpcOnlyError -> String requires Authenticated, Role(.admin)
}
