import k.auth.PasskeyAssertion
import k.auth.PasskeyCreationOptions
import k.auth.PasskeyCredential
import k.auth.PasskeyRegistration
import k.auth.PasskeyRequestOptions
import k.auth.PasskeyVerifier
import k.auth.SessionManager
import k.db.Database
import k.crypto.Password
import k.crypto.Uuid
import k.rpc.RpcOnlyError
import k.text.EmailAddress
import k.time.Clock

/// The server implementation of `AuthProtocol`: email-and-password accounts
/// on top of the `k.auth` session.
///
/// Emails are compared lower-cased in full, so `Ada@Example.com` and
/// `ada@example.com` are the same account.
public service AuthApi implements AuthProtocol {
    @Inject let accounts: AccountRepository
    @Inject let sessions: SessionManager
    @Inject let clock: Clock
    @Inject let verifier: PasskeyVerifier
    @Inject let passkeys: PasskeyRepository
    @Inject let database: Database

    rpc isEmailAvailable(email: EmailAddress) async throws RpcOnlyError -> Bool {
        let existing = await accounts.byEmail(email: normalized(email: email))

        return existing == nil
    }

    rpc signUp(request: SignUpRequest) async throws SignUpError {
        let passwordHash = await Password.hash(password: request.password)
        let account = Account(
            id: AccountId(value: Uuid.random()),
            email: normalized(email: request.email),
            passwordHash: passwordHash,
            createdAt: clock.now
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

    rpc passkeySignUpOptions() async throws RpcOnlyError -> PasskeyCreationOptions {
        return await verifier.registrationOptions()
    }

    rpc signUpWithPasskey(email: EmailAddress, registration: PasskeyRegistration) async throws SignUpError {
        // Every check (the session's challenge, the origin, the user
        // verification, the key) passes, or nothing is stored.
        let credential = try await verified(registration)
        let account = Account(
            id: AccountId(value: Uuid.random()),
            email: normalized(email: email),
            passwordHash: Password.unusableHash,
            createdAt: clock.now
        )

        // The account and its passkey are created together, or neither.
        try await database.transaction {
            do {
                try await accounts.insert(account)
            } catch .email {
                throw SignUpError.emailTaken
            }
            do {
                try await passkeys.insert(Passkey(
                    id: Uuid.random(),
                    accountId: account.id,
                    credentialId: credential.id,
                    publicKey: credential.publicKey,
                    signCount: credential.signCount,
                    userHandle: credential.userHandle,
                    backupEligible: credential.backupEligible,
                    backedUp: credential.backedUp,
                    createdAt: clock.now,
                    lastUsedAt: nil
                ))
            } catch .credentialId {
                throw SignUpError.passkeyNotAccepted
            }
        }

        await sessions.signIn(userId: account.id)
    }

    /// The verified credential of a registration, or `.passkeyNotAccepted`
    /// whichever check refused it: the browser learns nothing more.
    private func verified(_ registration: PasskeyRegistration) async throws SignUpError -> PasskeyCredential {
        do {
            return try await verifier.verifyRegistration(registration)
        } catch _ {
            throw SignUpError.passkeyNotAccepted
        }
    }

    rpc passkeySignInOptions() async throws RpcOnlyError -> PasskeyRequestOptions {
        return await verifier.signInOptions()
    }

    rpc signInWithPasskey(assertion: PasskeyAssertion) async throws SignInError {
        guard let passkey = await passkeys.byCredentialId(assertion.credentialId) else {
            throw SignInError.passkeyNotAccepted
        }

        do {
            let used = try await verifier.verifySignIn(assertion, credential: passkey.credential())
            await passkeys.recordUse(passkey, signCount: used.signCount, backedUp: used.backedUp, at: clock.now)
        } catch _ {
            throw SignInError.passkeyNotAccepted
        }

        await sessions.signIn(userId: passkey.accountId)
    }

    rpc signOut() async throws RpcOnlyError {
        await sessions.signOut()
    }
}

/// The form an address is stored and looked up in: the address without its
/// display name, lower-cased in full.
///
/// `EmailAddress.parse` has already trimmed it and lower-cased its domain.
/// It keeps the local part's case, because some mail servers treat it as
/// case-sensitive. This app lower-cases the local part too, so that two
/// people can't register `Ada@example.com` and `ada@example.com` as
/// different accounts.
func normalized(email: EmailAddress) -> String {
    return email.address.lowercased()
}
