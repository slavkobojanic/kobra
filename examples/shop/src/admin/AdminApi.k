import k.auth.SessionManager
import k.crypto.Password
import k.observe.Log
import store.OrderRepository
import store.OrderSummary

/// The server implementation of `AdminProtocol`.
///
/// Guards run before these methods, so `recentOrders` only ever executes for
/// a signed-in admin and contains no permission check of its own.
public service AdminApi implements AdminProtocol {
    @Inject let config: AdminConfig
    @Inject let sessions: SessionManager
    @Inject let orders: OrderRepository

    rpc signIn(request: SignInRequest) async throws SignInError {
        // Verify the password even when the email is wrong, so the response
        // time doesn't reveal whether an email address has an account.
        let passwordMatches = await Password.verify(password: request.password, hash: config.passwordHash)

        guard request.email.address.lowercased() == config.email.address.lowercased() else {
            throw SignInError.invalidCredentials
        }

        guard passwordMatches else {
            Log.warning("Failed admin sign-in for {config.email}")

            throw SignInError.invalidCredentials
        }

        await sessions.signIn(userId: StaffId(email: config.email), roles: Set([StaffRole.admin]))
    }

    rpc signOut() async {
        await sessions.signOut()
    }

    rpc recentOrders() async throws AdminError -> List<OrderSummary> {
        let recent = await orders.recent(limit: 50)

        return recent.map { order => order.summary() }
    }
}
