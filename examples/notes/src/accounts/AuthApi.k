import k.auth.Session
import k.crypto.Password
import k.crypto.Uuid
import k.db.Database

/// The server implementation of `AuthProtocol`: email-and-password accounts
/// on top of the `k.auth` session.
///
/// Emails are compared trimmed and lower-cased, so `Ada@Example.com ` and
/// `ada@example.com` are the same account.
public service AuthApi implements AuthProtocol {
    @Inject let accounts: AccountRepository
    @Inject let session: Session
    @Inject let database: Database

    rpc isEmailAvailable(email: String) async -> Bool {
        let existing = await accounts.byEmail(email: normalized(email: email))

        return existing == nil
    }

    rpc signUp(request: SignUpRequest) async throws SignUpError {
        let email = normalized(email: request.email)
        let passwordHash = await Password.hash(password: request.password)
        let account = Account(id: Uuid.random(), email: email, passwordHash: passwordHash)

        // Validation already checked availability, but another sign-up can
        // win the race. The unique index is the last line of defence; this
        // check turns the common case into a typed error.
        try await database.transaction {
            guard await accounts.byEmail(email: email) == nil else {
                throw SignUpError.emailTaken
            }

            await accounts.insert(account)
        }

        await session.signIn(userId: "{account.id}")
    }

    rpc signIn(credentials: Credentials) async throws SignInError {
        let account = await accounts.byEmail(email: normalized(email: credentials.email))

        // Verify against a hash even when no account has this email, so the
        // response time doesn't reveal which emails are registered.
        let hash = if let account { account.passwordHash } else { Password.unusableHash }
        let matches = await Password.verify(password: credentials.password, hash: hash)

        guard let account else {
            throw SignInError.invalidCredentials
        }

        guard matches else {
            throw SignInError.invalidCredentials
        }

        await session.signIn(userId: "{account.id}")
    }

    rpc signOut() async {
        await session.signOut()
    }
}

/// Trims and lower-cases an email address, so that two spellings of the same
/// address compare equal.
func normalized(email: String) -> String {
    return email.trimmed().lowercased()
}
