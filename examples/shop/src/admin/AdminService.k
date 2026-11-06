import store.OrderSummary

/// The admin pages' entry point to `AdminProtocol`.
public service AdminService {
    @Inject let api: AdminProtocol

    /// Signs in and starts a session cookie.
    ///
    /// - Parameter request: the sign-in form's value.
    /// - Throws: every `SignInError`; see `AdminProtocol.signIn(request:)`.
    func signIn(request: SignInRequest) async throws SignInError {
        try await api.signIn(request: request)
    }

    /// Ends the current session.
    func signOut() async {
        await api.signOut()
    }

    /// Lists the 50 most recent orders, newest first.
    ///
    /// - Throws: `AdminError.rpcFailure` unless the caller is a signed-in
    ///   admin.
    func recentOrders() async throws AdminError -> List<OrderSummary> {
        return try await api.recentOrders()
    }
}
