import k.auth.PasskeyAssertion
import k.auth.PasskeyCreationOptions
import k.auth.PasskeyRegistration
import k.auth.PasskeyRequestOptions
import k.auth.PasskeyVerificationError
import k.auth.PasskeyVerifier
import k.auth.Session
import k.auth.SessionManager
import k.rpc.RpcOnlyError

/// The server side of `AuthProtocol`.
public service AuthApi implements AuthProtocol {
    @Inject let verifier: PasskeyVerifier
    @Inject let sessions: SessionManager
    @Inject let session: Session
    @Inject let book: PasskeyBook

    rpc signUpOptions() async throws RpcOnlyError -> PasskeyCreationOptions {
        return await verifier.registrationOptions()
    }

    rpc signUp(name: String, registration: PasskeyRegistration) async throws RpcOnlyError -> String {
        do {
            let credential = try await verifier.verifyRegistration(registration)
            guard book.find(id: credential.id) == nil else {
                return "already stored"
            }
            book.add(owner: name, credential: credential)
            await sessions.signIn(userId: AccountId(name: name))
            return "signed up"
        } catch let refusal {
            return describe(refusal)
        }
    }

    rpc signInOptions() async throws RpcOnlyError -> PasskeyRequestOptions {
        return await verifier.signInOptions()
    }

    rpc signIn(assertion: PasskeyAssertion) async throws RpcOnlyError -> String {
        guard let stored = book.find(id: assertion.credentialId) else {
            return "unknown passkey"
        }
        do {
            let used = try await verifier.verifySignIn(assertion, credential: stored.credential)
            book.update(used)
            await sessions.signIn(userId: AccountId(name: stored.owner))
            return "signed in as {stored.owner}"
        } catch let refusal {
            return describe(refusal)
        }
    }

    rpc whoAmI() async throws RpcOnlyError -> String {
        if let user = session.user {
            return user.id.name
        }
        return ""
    }

    rpc signOut() async throws RpcOnlyError {
        await sessions.signOut()
    }
}

/// What the tests read for each refusal.
func describe(_ refusal: PasskeyVerificationError) -> String {
    match refusal {
        case .challengeMismatch { return "challenge mismatch" }
        case .originMismatch { return "origin mismatch" }
        case .rpIdMismatch { return "rp id mismatch" }
        case .userNotVerified { return "user not verified" }
        case .unsupportedAlgorithm { return "unsupported algorithm" }
        case .invalidSignature { return "invalid signature" }
        case .signCountRegressed { return "sign count regressed" }
        case .credentialMismatch { return "credential mismatch" }
        case .malformed { return "malformed" }
    }
}
