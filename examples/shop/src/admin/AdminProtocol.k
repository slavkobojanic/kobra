import k.auth.Authenticated
import k.auth.RateLimit
import k.auth.Role
import k.rpc.RpcOnlyError
import store.OrderSummary

/// The contract for staff: signing in, and seeing orders.
///
/// Guards are declared per rpc. Signing in is open to anyone but limited to
/// five attempts a minute per client; everything else requires a signed-in
/// admin. The guards are part of the contract, so clients can see them too.
public protocol AdminProtocol {
    /// Signs in and starts a session cookie.
    ///
    /// - Parameter request: the sign-in form.
    /// - Throws: `SignInError.invalidCredentials` when the email or password
    ///   is wrong, and `.rpcFailure` when the request is invalid, after too
    ///   many attempts, or when the browser can't reach the server.
    rpc signIn(request: SignInRequest) throws SignInError requires RateLimit(5.perMinute)

    /// Ends the current session. Signing out when not signed in does nothing.
    ///
    /// - Throws: `RpcOnlyError.rpcFailure` when the browser can't reach the
    ///   server.
    rpc signOut() throws RpcOnlyError

    /// Lists the 50 most recent orders, newest first.
    ///
    /// - Throws: `RpcOnlyError.rpcFailure` unless the caller is a signed-in
    ///   admin, or when the browser can't reach the server.
    rpc recentOrders() throws RpcOnlyError -> List<OrderSummary> requires Authenticated, Role(.admin)
}
