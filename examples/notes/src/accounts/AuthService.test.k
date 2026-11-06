import k.auth.Session

// Integration tests: no `use` rules, so these run the real AuthService,
// AuthProtocol in-process, AuthApi and a rolled-back SQLite database.
test AuthService {
    @Inject let auth: AuthService
    @Inject let session: Session

    test "signing up signs the new account in" {
        try await auth.signUp(request: signUpRequest(email: "ada@example.com"))

        expect auth.user != nil
        expect auth.user == session.user
    }

    test "signing in matches the email in any letter case and with spaces" {
        try await auth.signUp(request: signUpRequest(email: "Ada@Example.com"))
        await auth.signOut()

        try await auth.signIn(credentials: Credentials(email: " ADA@example.COM ", password: testPassword))

        expect auth.user != nil
    }

    test "a wrong password is refused and leaves the visitor signed out" {
        try await auth.signUp(request: signUpRequest(email: "ada@example.com"))
        await auth.signOut()

        expect throws SignInError.invalidCredentials {
            try await auth.signIn(credentials: Credentials(email: "ada@example.com", password: "not the password"))
        }

        expect auth.user == nil
    }

    test "an unknown email is refused exactly like a wrong password" {
        expect throws SignInError.invalidCredentials {
            try await auth.signIn(credentials: Credentials(email: "nobody@example.com", password: testPassword))
        }
    }

    test "an email stops being available once an account uses it, in any letter case" {
        let before = await auth.emailAvailable(email: "ada@example.com")

        try await auth.signUp(request: signUpRequest(email: "ada@example.com"))

        let after = await auth.emailAvailable(email: " Ada@Example.com")

        expect before
        expect not after
    }

    test "signing out ends the session, and signing out again does nothing" {
        try await auth.signUp(request: signUpRequest(email: "ada@example.com"))

        await auth.signOut()
        await auth.signOut()

        expect auth.user == nil
    }
}

/// A password that satisfies `SignUpRequest`'s rules. Shared by this
/// module's tests.
let testPassword = "correct horse battery"

/// A valid sign-up request for `email`, using `testPassword`. Shared by this
/// module's tests.
func signUpRequest(email: String) -> SignUpRequest {
    return SignUpRequest(email: email, password: testPassword, confirmPassword: testPassword)
}
