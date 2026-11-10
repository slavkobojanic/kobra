import k.auth.Session
import k.test.TestAuthenticator
import k.text.EmailAddress

// Integration tests: the real PasskeyService, AuthProtocol in-process,
// AuthApi with the real PasskeyVerifier, a rolled-back SQLite database,
// and `TestAuthenticator`, which signs real assertions in place of the
// device.
test PasskeyService {
    @Inject let passkeys: PasskeyService
    @Inject let auth: AuthService
    @Inject let session: Session
    @Inject let authenticator: TestAuthenticator

    test "signing up with a passkey signs the new account in" {
        await passkeys.prepareSignUp()

        expect await passkeys.signUp(email: EmailAddress("ada@example.com")) == nil
        expect session.user != nil
        expect authenticator.passkeys.count == 1
    }

    test "the passkey signs the account in again, and no password does" {
        await passkeys.prepareSignUp()
        expect await passkeys.signUp(email: EmailAddress("ada@example.com")) == nil
        let ada = session.user
        try await auth.signOut()

        await passkeys.prepareSignIn()
        expect await passkeys.signIn() == nil
        expect session.user == ada

        try await auth.signOut()
        expect throws SignInError.invalidCredentials {
            try await auth.signIn(credentials: Credentials(email: EmailAddress("ada@example.com"), password: testPassword))
        }
    }

    test "an email that already has an account is refused" {
        try await auth.signUp(request: signUpRequest(email: EmailAddress("ada@example.com")))
        try await auth.signOut()
        await passkeys.prepareSignUp()

        expect await passkeys.signUp(email: EmailAddress("Ada@example.com")) == "An account with this email already exists. Sign in instead."
        expect session.user == nil
    }

    test "a passkey made on another site isn't accepted" {
        authenticator.setOrigin("https://k-notes.example.evil")
        await passkeys.prepareSignUp()

        expect await passkeys.signUp(email: EmailAddress("ada@example.com")) == "That passkey wasn't accepted. Try again."
        expect session.user == nil
    }

    test "a dismissed sheet changes nothing, and a new challenge is ready" {
        await passkeys.prepareSignUp()
        let first = passkeys.signUpOptions
        authenticator.dismissNext()

        expect await passkeys.signUp(email: EmailAddress("ada@example.com")) == "No passkey was made. Try again when you're ready."
        expect passkeys.signUpOptions != first
        expect await passkeys.signUp(email: EmailAddress("ada@example.com")) == nil
    }

    test "a device without a K Notes passkey is told to use the password" {
        await passkeys.prepareSignIn()

        expect await passkeys.signIn() == "No passkey was used. If this device has none for K Notes, sign in with your password."
    }

    test "a cloned passkey's counter is refused" {
        await passkeys.prepareSignUp()
        expect await passkeys.signUp(email: EmailAddress("ada@example.com")) == nil
        try await auth.signOut()
        authenticator.setSignCount(3)
        await passkeys.prepareSignIn()
        expect await passkeys.signIn() == nil
        try await auth.signOut()

        authenticator.setSignCount(2)

        expect await passkeys.signIn() == "That passkey wasn't accepted."
        expect session.user == nil
    }
}
