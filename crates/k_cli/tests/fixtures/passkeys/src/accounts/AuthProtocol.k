import k.auth.PasskeyAssertion
import k.auth.PasskeyCreationOptions
import k.auth.PasskeyRegistration
import k.auth.PasskeyRequestOptions
import k.rpc.RpcOnlyError

/// Signing up and in with a passkey. Each verifying rpc answers what
/// happened as text, so the tests read it.
public protocol AuthProtocol {
    /// A registration challenge for a new account.
    rpc signUpOptions() throws RpcOnlyError -> PasskeyCreationOptions

    /// Verifies the registration, makes the account and signs it in.
    rpc signUp(name: String, registration: PasskeyRegistration) throws RpcOnlyError -> String

    /// A sign-in challenge.
    rpc signInOptions() throws RpcOnlyError -> PasskeyRequestOptions

    /// Verifies the assertion and signs its account in.
    rpc signIn(assertion: PasskeyAssertion) throws RpcOnlyError -> String

    /// Who is signed in, or `""`.
    rpc whoAmI() throws RpcOnlyError -> String

    /// Signs out.
    rpc signOut() throws RpcOnlyError
}
