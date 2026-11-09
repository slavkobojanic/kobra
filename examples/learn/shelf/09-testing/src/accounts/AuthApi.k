import k.auth.SessionManager
import k.crypto.Password
import k.crypto.Uuid
import k.rpc.RpcOnlyError
import k.time.Clock

/// The server's side of `AuthProtocol`.
public service AuthApi implements AuthProtocol {
    @Inject let accounts: AccountRepository
    @Inject let sessions: SessionManager
    @Inject let clock: Clock

    rpc signUp(credentials: Credentials) async throws SignUpError {
        let account = Account(
            id: AccountId(value: Uuid.random()),
            email: credentials.email.address.lowercased(),
            passwordHash: await Password.hash(password: credentials.password),
            createdAt: clock.now
        )

        do {
            try await accounts.insert(account)
        } catch .email {
            throw SignUpError.emailTaken
        }

        await sessions.signIn(userId: account.id)
    }

    rpc signIn(credentials: Credentials) async throws SignInError {
        let account = await accounts.byEmail(email: credentials.email.address.lowercased())

        // Check a hash even when there's no account, so the time the answer
        // takes doesn't tell which addresses have one.
        let hash = if let account { account.passwordHash } else { Password.unusableHash }
        let matches = await Password.verify(password: credentials.password, hash: hash)

        guard let account, matches else {
            throw SignInError.invalidCredentials
        }

        await sessions.signIn(userId: account.id)
    }

    rpc signOut() async throws RpcOnlyError {
        await sessions.signOut()
    }
}
