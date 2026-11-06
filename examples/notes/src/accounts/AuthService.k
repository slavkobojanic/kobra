import k.auth.Session
import k.auth.SessionUser

/// The browser's entry point for accounts: signing up, in and out, and who
/// is signed in.
///
/// Pages and the layout use this service instead of `AuthProtocol`. It holds
/// no state of its own: who is signed in comes from `Session`, which every
/// successful call here updates.
public service AuthService {
    @Inject let api: AuthProtocol
    @Inject let session: Session

    /// The signed-in person, or `nil` for a guest.
    @Derived public let user: SessionUser? = session.user

    /// Reports whether an email address is free to register.
    ///
    /// The `@Check` on `SignUpRequest.email` refers to this function: the
    /// form calls it debounced on blur, and the server calls it again before
    /// `signUp` runs.
    ///
    /// - Parameter email: the address as typed.
    /// - Returns: `true` when no account uses the address.
    public func emailAvailable(email: String) async -> Bool {
        return await api.isEmailAvailable(email: email)
    }

    /// Creates an account and signs it in.
    ///
    /// - Parameter request: a validated sign-up request.
    /// - Throws: every `SignUpError`; see `AuthProtocol.signUp(request:)`.
    public func signUp(request: SignUpRequest) async throws SignUpError {
        try await api.signUp(request: request)
    }

    /// Signs in with an email address and password.
    ///
    /// - Parameter credentials: a validated sign-in form.
    /// - Throws: every `SignInError`; see `AuthProtocol.signIn(credentials:)`.
    public func signIn(credentials: Credentials) async throws SignInError {
        try await api.signIn(credentials: credentials)
    }

    /// Signs out. Does nothing for a guest.
    public func signOut() async {
        await api.signOut()
    }
}
