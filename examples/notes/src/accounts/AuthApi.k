import k.auth.SessionManager
import k.crypto.Password
import k.crypto.Uuid

/// The server implementation of `AuthProtocol`: email-and-password accounts
/// on top of the `k.auth` session.
///
/// Emails are compared trimmed and lower-cased, so `Ada@Example.com ` and
/// `ada@example.com` are the same account.
public service AuthApi implements AuthProtocol {
    @Inject let accounts: AccountRepository
    @Inject let sessions: SessionManager

    rpc isEmailAvailable(email: String) async -> Bool {
        let existing = await accounts.byEmail(email: normalized(email: email))

        return existing == nil
    }

    rpc signUp(request: SignUpRequest) async throws SignUpError {
        let passwordHash = await Password.hash(password: request.password)
        let account = Account(
            id: AccountId(value: Uuid.random()),
            email: normalized(email: request.email),
            passwordHash: passwordHash
        )

        // The email's `@Check` already ran, but another sign-up can register
        // the same address before this insert. The unique index decides,
        // and its typed violation becomes this rpc's error.
        do {
            try await accounts.insert(account)
        } catch .email {
            throw SignUpError.emailTaken
        }

        await sessions.signIn(userId: account.id)
    }

    rpc signIn(credentials: Credentials) async throws SignInError {
        let account = await accounts.byEmail(email: normalized(email: credentials.email))

        // Verify against a hash even when no account has this email, so the
        // response time doesn't reveal which emails are registered.
        let hash = if let account { account.passwordHash } else { Password.unusableHash }
        let matches = await Password.verify(password: credentials.password, hash: hash)

        guard let account, matches else {
            throw SignInError.invalidCredentials
        }

        await sessions.signIn(userId: account.id)
    }

    rpc signOut() async {
        await sessions.signOut()
    }
}

/// Trims and lower-cases an email address, so that two spellings of the same
/// address compare equal.
func normalized(email: String) -> String {
    return email.trimmed().lowercased()
}
