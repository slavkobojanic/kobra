# Passkeys

## Decision

**A passkey is two stdlib services, one per side, and the app's own entity.** The browser runs the WebAuthn ceremonies through **`Passkeys`** (client-only, `k.auth`), the server checks what comes back with **`PasskeyVerifier`** (server-only, `k.auth`), and the app stores each credential in an entity of its own and signs the person in with the existing `SessionManager.signIn(userId:)` ([26-auth-and-sessions.md](26-auth-and-sessions.md)). This is the WebAuthn row of [45's inventory](45-web-platform.md#inventory), Tier 2, and the passkey half of [26's open question](26-auth-and-sessions.md#open-questions).

Four rules hold everywhere:

- **No third-party account and no third-party trust.** The relying party is the app itself: its id is the host of `K_PUBLIC_ORIGIN`, its origin `K_PUBLIC_ORIGIN`. The server asks for **attestation `"none"`** and never verifies an attestation statement, so no vendor's metadata service, root certificates or account is involved ([why](#why)).
- **Every challenge is the session's, single-use and short-lived.** A challenge is issued by the server into the current session (as `SessionManager.issueState()` issues an OAuth state), is consumed by the first verification that presents it, whether that verification succeeds or not, and expires after ten minutes. The ceremony (registration or sign-in) and, for a registration, the user handle are bound into it.
- **Verification is all or nothing, in Rust.** The origin, the RP id hash, the user-present and user-verified flags, the algorithm, the signature and the signature counter are checked on the server with `ring`, in one op, and a failure is one typed case of `PasskeyVerificationError`. There is no partial result an app could use by mistake.
- **Only the click shows the browser's passkey sheet.** `register` and `signIn` are `@UserAction` ([45, "User activation"](45-web-platform.md#user-activation)): Safari refuses WebAuthn outside a user's action, and the sheet is a prompt like any other. So the options (with their challenge) are fetched before the click, from an effect, never after an `await` in the click.

```k
// examples/notes/src/accounts/PasskeyService.k (shortened)
public service PasskeyService {
    @Inject let passkeys: Passkeys
    @Inject let api: AuthProtocol

    @State var signInOptions: PasskeyRequestOptions? = nil

    // From the page's effect: the challenge is in hand before the click.
    func prepareSignIn() async {
        do {
            signInOptions = try await api.passkeySignInOptions()
        } catch .rpcFailure {
            signInOptions = nil
        }
    }

    // From the click: the sheet opens before any other await.
    func signIn() async -> String? {
        guard let options = signInOptions else { return "Passkeys aren't ready yet." }
        var problem: String? = nil
        do {
            let assertion = try await passkeys.signIn(options)       // @UserAction
            problem = await signInWith(assertion: assertion)          // the rpc, in a do of its own
        } catch let refusal {
            problem = deviceProblem(refusal, making: false)
        }
        if problem != nil {
            await prepareSignIn()                                     // the challenge was used
        }
        return problem
    }
}
```

```k
// examples/notes/src/accounts/AuthApi.k
public service AuthApi implements AuthProtocol {
    @Inject let verifier: PasskeyVerifier
    @Inject let sessions: SessionManager
    @Inject let passkeys: PasskeyRepository

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
}
```

## The browser: `Passkeys`

```k
@Platform(side: .client)
public interface Passkeys {
    var isSupported: Bool

    @UserAction
    func register(_ options: PasskeyCreationOptions, name: String) async throws PasskeyError -> PasskeyRegistration

    @UserAction
    func signIn(_ options: PasskeyRequestOptions) async throws PasskeyError -> PasskeyAssertion
}

public enum PasskeyError implements Error, Equatable {
    case unsupported           // no WebAuthn here, or not a secure context
    case cancelled             // dismissed, timed out, or no passkey for this site on the device
    case alreadyRegistered     // the device already holds one of the excluded credentials
    case failed                // anything else, such as an RP id the page's origin can't use
}
```

- **`register(_:name:)`** calls `navigator.credentials.create` with the server's options: the challenge, `rp` (`id` and the app's name), the user handle, `name` as both the user's `name` and `displayName` (the label the device shows; usually the email typed), ES256, EdDSA and RS256 in that order of preference, `residentKey: "required"` (a passkey is a discoverable credential), `userVerification: "required"`, `attestation: "none"`, the excluded credentials, and a five-minute timeout. `name` is the client's because a sign-up form's email is typed after the options were fetched; the server never relies on it.
- **`signIn(_:)`** calls `navigator.credentials.get` with the challenge, the RP id, `userVerification: "required"` and the allowed credentials (none: the device offers every passkey it holds for the site).
- **`NotAllowedError` is `.cancelled`.** Browsers report a dismissed sheet, a timeout and "no passkey here" with the same error, on purpose (so a site can't probe which passkeys a device holds), so Kobra doesn't pretend to tell them apart.
- **The results are values the server reads:** `PasskeyRegistration` and `PasskeyAssertion` hold the browser's bytes as base64url text and are `Codable`, so they cross an rpc unchanged.
- **Client-only, like every platform interface that needs the user;** `isSupported` may be read while the server renders (`false`). `Passkeys` asks for no permission, so it has no `permission` member ([45, "Permissions"](45-web-platform.md#permissions)): the sheet is the consent, every time.

## The server: `PasskeyVerifier`

```k
@ServerOnly
public service PasskeyVerifier {
    func registrationOptions(userHandle: String? = nil, excluding: List<String> = []) async -> PasskeyCreationOptions
    func verifyRegistration(_ registration: PasskeyRegistration) async throws PasskeyVerificationError -> PasskeyCredential

    func signInOptions(allowing: List<String> = []) async -> PasskeyRequestOptions
    func verifySignIn(_ assertion: PasskeyAssertion, credential: PasskeyCredential) async throws PasskeyVerificationError -> PasskeyCredential
}

public enum PasskeyVerificationError implements Error, Equatable {
    case challengeMismatch     // not issued to this session, already used, expired, or for the other ceremony
    case originMismatch        // clientDataJSON's origin isn't K_PUBLIC_ORIGIN, or the ceremony ran in a frame
    case rpIdMismatch          // the authenticator signed for another RP id
    case userNotVerified       // the user-present or user-verified flag is missing
    case unsupportedAlgorithm  // the credential's key isn't ES256, EdDSA (Ed25519) or RS256
    case invalidSignature
    case signCountRegressed    // the counter didn't increase: a cloned authenticator
    case credentialMismatch    // the assertion is for another credential or user handle
    case malformed             // anything that doesn't parse: CBOR, authenticator data, JSON, base64url
}
```

- **`registrationOptions(userHandle:excluding:)`** issues a registration challenge into the current session (a guest gets a guest session, as for `issueState()`). The user handle is the app's for an account that already has passkeys, or a fresh random 32 bytes when `nil` (a sign-up). `excluding` lists credential ids the device must not register again (the account's passkeys).
- **`verifyRegistration(_:)`** consumes the challenge, checks the client data (type `webauthn.create`, the challenge, the origin, no `crossOrigin`, no `topOrigin`), the attestation object's authenticator data (the RP id hash, the flags, an attested credential whose id is the one the browser reported, a COSE key of a supported algorithm, nothing left over), and returns the **`PasskeyCredential`** to store. The attestation statement is not read, whatever its format ([why](#why)).
- **`signInOptions(allowing:)`** issues a sign-in challenge; `allowing` is empty for passkeys (the device chooses), or an account's credential ids for a second-factor style prompt.
- **`verifySignIn(_:credential:)`** consumes the challenge, checks the client data (type `webauthn.get`), the authenticator data (RP id hash, flags, no attested credential), the signature over `authenticatorData ‖ SHA-256(clientDataJSON)` under the stored key, the credential id and user handle against the stored credential, and the counter, and returns the credential with its new counter and backup state, which the app writes back.
- **User verification is required.** A passkey replaces a password, so the device must have checked the person (biometrics, PIN), not only their presence. Both flags are required at registration and at every sign-in.
- **The counter rule** is WebAuthn's: when the stored or the new counter is nonzero, the new one must be greater, else `.signCountRegressed`. Synced passkeys (iCloud Keychain, Google Password Manager) always report `0`, which passes.
- **`PasskeyVerifier` is a plain stdlib service, not an interface:** there is one way to verify, and in tests the real verifier checks what `TestAuthenticator` signs ([tests](#tests)), so no recording double is needed.

## Values

```k
public struct PasskeyCreationOptions implements Equatable, Codable {
    let challenge: String       // base64url
    let rpId: String
    let rpName: String
    let userHandle: String      // base64url, up to 64 bytes
    let excluded: List<String>  // credential ids, base64url
}

public struct PasskeyRequestOptions implements Equatable, Codable {
    let challenge: String
    let rpId: String
    let allowed: List<String>
}

public struct PasskeyRegistration implements Equatable, Codable {
    let credentialId: String
    let clientDataJSON: String
    let attestationObject: String
}

public struct PasskeyAssertion implements Equatable, Codable {
    let credentialId: String
    let clientDataJSON: String
    let authenticatorData: String
    let signature: String
    let userHandle: String?
}

public struct PasskeyCredential implements Equatable, Codable {
    let id: String              // the credential id, base64url
    let publicKey: String       // the COSE_Key, base64url, as the authenticator wrote it
    let signCount: Int
    let userHandle: String
    let backupEligible: Bool    // a synced passkey
    let backedUp: Bool
}
```

- **Everything is base64url text,** like `PushSubscription`, so the values are `Codable` with no new wire type, their fields are ordinary text columns, and `==` needs nothing special.
- **The public key stays a COSE key,** not one of `k.crypto`'s key types: a credential may be any of three algorithms, and the verifier is the only reader. [33's open question](33-crypto.md#open-questions) expected `EcdsaP256PublicKey` and `Ed25519PublicKey` here; the COSE text is simpler to store and also covers RS256, which `RsaPublicKey`'s 2,048-bit minimum and SPKI form don't fit as naturally.

## Storage: an entity the app owns

An entity column can't hold a struct (K6002), so the app's entity has the credential's fields, and the app converts both ways:

```k
public entity Passkey {
    @Id let id: Uuid
    let accountId: AccountId
    @Unique let credentialId: String
    let publicKey: String
    var signCount: Int
    let userHandle: String
    let backupEligible: Bool
    var backedUp: Bool
    let createdAt: Date
    var lastUsedAt: Date?
}
```

- **The credential id is unique,** so a second registration of the same credential (a replay of a registration within its challenge's lifetime is already refused) can't attach it to another account.
- **The stdlib keeps no passkey table.** Accounts, labels, how many passkeys an account may have and what removing the last one means are the app's decisions; the session table (`k_session`) is the only state `k.auth` keeps, and it holds the challenge.
- **Signing in is the app's `sessions.signIn(userId:)`** after a successful `verifySignIn`, exactly as after a password check. A sign-up verifies the registration, creates the account and its `Passkey` row in one transaction, then signs in.

## Challenges

- **The challenge is 32 random bytes, a ceremony byte (`1` registration, `2` sign-in) and, for a registration, the user handle,** and its base64url text is stored (hashed) as the session's state, through the same row and lifetime as `issueState()`. Verification reads the challenge back from the client data, consumes it with the same single-use rule as `consumeState`, and only then reads the ceremony and the user handle from it, so neither can be changed by the browser.
- **One challenge per session at a time.** Issuing one replaces the session's previous challenge or OAuth state; a sign-in page and a sign-up page open in two tabs of one browser share a session, so the older one's attempt fails with `.challengeMismatch`, and its page fetches new options and asks again.

## Tests

**`TestAuthenticator`** (from `k.test`, the default for `Passkeys` in `k test`) is a working authenticator: it holds real key pairs (key `n` of `TestKeys.ecdsaP256` and `TestKeys.ed25519`, and a fixed 2,048-bit RSA key), writes real CBOR attestation objects and authenticator data, and signs real assertions, so the server's real `PasskeyVerifier` checks what it produces.

| Member | Meaning |
|---|---|
| `isSupported`, `setSupported(_:)` | the browser's support |
| `passkeys: List<String>` | the credential ids it holds, oldest first |
| `prompts: Int` | how many sheets it showed |
| `useAlgorithm(_ algorithm: PasskeyAlgorithm)` | the key type of the next registration: `.es256` (the default), `.edDSA` or `.rs256` |
| `dismissNext()` | the next ceremony throws `.cancelled`, as a dismissed sheet does |
| `setUserVerified(_:)` | `false` leaves the UV flag unset, as a security key without a PIN does |
| `setOrigin(_:)` | the origin it writes into the client data: a phishing page's, for tests |
| `setSignCount(_:)` | the counter of the next assertion, to play a cloned authenticator |
| `forgetPasskeys()` | the device holds none: `signIn` throws `.cancelled` |

```k
// crates/k_cli/tests/fixtures/passkeys/src/accounts/PasskeyFlow.test.k
test PasskeyFlow {
    @Inject let authenticator: TestAuthenticator
    @Inject let flow: PasskeyFlow
    @Inject let auth: AuthProtocol

    test "a passkey from another origin is refused" {
        await signedUpThenOut("ada")
        authenticator.setOrigin("https://notes.example.evil")

        expect await flow.signIn() == "origin mismatch"
        expect try await auth.whoAmI() == ""
    }
}
```

**The stdlib's own tests:**

- **WebAuthn Level 3's test vectors** (§16): ES256 with no attestation and with self attestation, `crossOrigin` and `topOrigin` client data (refused), RS256 and Ed25519 with packed attestation (accepted with the statement unread), each registration and its authentication, on the Rust side.
- **Malformed and tampered input,** each a typed case: every byte of the authenticator data and signature flipped, truncations at every length, trailing bytes, indefinite-length and over-deep CBOR, duplicate map keys, a COSE key of an unsupported algorithm or with a point off the curve, a credential id over 1,023 bytes, a BS flag without BE, a changed challenge, type, origin or RP id hash, and a decreasing counter.
- **`k test` suites** on a fixture: sign-up, sign-in, each algorithm, a dismissed sheet, an excluded credential, a missing UV flag, a wrong origin, a replayed and a replaced challenge, a challenge from the other ceremony, a regressed counter, a swapped user handle and a forged signature.
- **Real Chrome,** with DevTools' `WebAuthn.addVirtualAuthenticator` (CTAP2, internal transport, resident keys and user verification): registering, signing in after signing out, and a page on another origin than `K_PUBLIC_ORIGIN` refused.

## Server and client

| Declaration | Side |
|---|---|
| `Passkeys`, `BrowserPasskeys` | client-only |
| `PasskeyCreationOptions`, `PasskeyRequestOptions`, `PasskeyRegistration`, `PasskeyAssertion`, `PasskeyCredential`, `PasskeyError`, `PasskeyAlgorithm` | both (values) |
| `PasskeyVerifier`, `PasskeyVerificationError` | server-only (the error is a value, but only the verifier throws it) |
| `TestAuthenticator` | `k.test` |

## Example

**`examples/notes` offers passkeys next to the password.** The sign-up page gains "Sign up with a passkey" (the email field, then the device's sheet; the account gets an unusable password hash), and the sign-in page "Sign in with a passkey". The account's passkeys are `Passkey` rows (migration `0004`).

## As built

Built tests first, as designed above, with these decisions and changes:

- **Declarations.** `k.auth` holds `Passkeys`, `BrowserPasskeys`, `PasskeyVerifier`, `PasskeyError`, `PasskeyVerificationError`, `PasskeyAlgorithm` and the five value structs; `k.test` holds `TestAuthenticator`. `Passkeys` binds to `BrowserPasskeys` with no `use` rule (`client_containers` in `k_codegen`'s `app.rs`), `TestAuthenticator` replaces it in `k test` (`@TestDefault`), and `PasskeyVerifier` is a server-only stdlib service (`$s.passkeyVerifier($r)`, in `server_platform_service`), injected from a test's server container (`suite.rs`' `SERVER_STDLIB`). **No new diagnostic code:** a component injecting `Passkeys` is the existing client-only rule, a ceremony outside a click is K4030, `PasskeyVerifier` in client code is the server-only rule, and unhandled errors are K2206/K2203.
- **The Rust side is `crates/k_server/src/passkey.rs`:** the CBOR reader (definite lengths only, no tags or floats, no duplicate map keys, at most 8 levels), `parse_auth_data`, `parse_cose_key` (ES256 with the point checked on the curve by `p256`, Ed25519, RS256 with 2,048 to 8,192-bit moduli), `verify_registration` and `verify_assertion`, and the ops `op_k_passkey_verify_registration` and `_assertion` (JSON in, `{ok}` or `{error}` out). ES256 signatures are DER (`ring`'s `ECDSA_P256_SHA256_ASN1`), as WebAuthn writes them. No new crate.
- **Challenges are `op_k_session_issue_challenge`,** a sibling of `op_k_session_issue_state` (`ProcessState::session_issue_challenge`, the same row, hash and lifetime, with the extra bytes after the 32 random ones), and are taken by the existing `op_k_session_consume_state`. The JavaScript side (`app.js`' `passkeyVerifier`) reads the challenge from the client data, takes it, checks the ceremony byte, then calls the op. A guest's new session now also reaches a test's later requests (`sessionChanged`), which `issueState()` lacked as well and now has.
- **`rpName` is the RP id,** not the app's title: browsers no longer show `rp.name` in their passkey sheets, and the server runtime has no app title at hand. No `PasskeyConfig`: the RP id and origin come from `K_PUBLIC_ORIGIN` ([open questions](#open-questions)). The server has no `URL`, so the host is cut from the origin's text.
- **`TestAuthenticator`'s ceremonies are Rust too** (`op_k_passkey_test_register` and `_assert`, `k_server::passkey::test_register` and `test_assert`): the kit keeps the credentials (`{id, algorithm, key, userHandle, rpId}`), and Rust writes the CBOR and signs. Key `n` (the `n`th passkey made) is `TestKeys.ecdsaP256(n)`'s or `TestKeys.ed25519(n)`'s, by the same derivation; RS256 uses one fixed 2,048-bit PKCS #1 key, `crates/k_server/src/passkey/test-rsa-2048.der`, since `ring` can't generate RSA keys. The credential id is 16 bytes of the SHA-256 of `k.test.TestAuthenticator/<algorithm>/<n>`. Its counter starts at 0 and counts each assertion; `setSignCount(n)` makes the next one `n`. Flags: UP, UV unless `setUserVerified(false)`, never BE or BS.
- **Vectors:** WebAuthn Level 3 §16.2–16.6, 16.10 and 16.11 (`crates/k_server/src/passkey/webauthn-vectors.json`, extracted from the editor's draft): `none.ES256`, `packed-self.ES256`, the long credential id, `packed.RS256` (a 3,482-bit modulus) and `packed.EdDSA` register and authenticate; `crossOrigin` and `topOrigin` are refused with `.originMismatch`. Their flags leave UV unset, so they run with user verification not required, and one check shows the production setting refusing them. Not run: the packed ES384, ES512 and Ed448 vectors (algorithms Kobra doesn't take; a COSE key of each is refused as `.unsupportedAlgorithm` in the unit tests), TPM, Android Key, Apple and FIDO U2F (attestation Kobra never reads).
- **The `passkeys` fixture keeps credentials in a `@Scope(.server)` list,** not an entity, so it needs no database; the notes example shows the entity.
- **One `do` catches the device's errors and the server's.** The generated code first told a `catch` case by its position only, so `SignUpError.passkeyNotAccepted` (case 1) was caught as `PasskeyError.cancelled` (case 1), and the notes example and the fixture called the rpc from a helper with its own `do`. Now a `do` (or `expect throws`) whose body throws more than one error type records which type each `try` and `throw` had (`$et`, the error enum's declaration index; [codegen.md §7](../compiler/codegen.md#7-errors-panics-and-typed-throws)), and each clause checks the type as well as the case, so both call the device and the server in one `do` with a clause per case. Matching an `Error`-typed value (a catch-all over two types) against a case is K2312, since the value doesn't say which enum it is. Run cases: `tests/corpus/run/errors/two-enums-*.k`.
- **The chunk** is `passkeys.[hash].js` (part `Passkeys`, `PASSKEYS_SIZE_BUDGET` of 1.25 KB gzip: 1,055 B compacted on its own, 996 B as notes ships it). The shipped full client runtime didn't grow. **Notes' shipped client grew from 36,246 B to 39,132 B**, within its 40,000 B budget, which didn't change. `PasskeySignUp` and `PasskeySignIn` are `@Rendering(.client)` and show their button once the options are fetched, so a click always has a challenge to use. The options rpc gives a first-time visitor a guest session, which first shared the session cookie: a password sign-in sent before that answer arrived could have its cookie overwritten by the guest's, and the notes Chrome tests waited for the passkey button before signing in or up. The guest session now has a cookie of its own ([26, "Guest sessions"](26-auth-and-sessions.md#guest-sessions)), and the tests no longer wait.
- **Real Chrome** (`crates/k_cli/tests/chrome.rs`): `passkeys_sign_up_and_in_with_a_virtual_authenticator_in_chrome` adds a CTAP2 internal authenticator with resident keys and user verification over DevTools, signs up, out and in through the fixture's buttons, and reads the resident credential and its counter back; `passkeys_refuse_a_page_on_another_origin_in_chrome` opens the fixture on `127.0.0.1` while its public origin is `localhost`, and Chrome refuses the ceremony (`.failed`) with no credential made.
- **Not built:** autofill (conditional mediation), for want of an `autocomplete` modifier ([open questions](#open-questions)); adding a passkey to a password account, and listing or removing passkeys, in the notes example.

## Why

- **Attestation `"none"` only.** Attestation tells a relying party which make of authenticator holds the key. Checking it needs the FIDO Metadata Service (a third party's signed list, fetched and refreshed) or a hand-kept set of vendor roots, and the synced passkeys most people now have (Apple, Google, Microsoft) don't attest at all. For an app that only needs "the same device as last time", the attestation adds a dependency and a privacy cost (it can identify the device model) and no protection, so Kobra asks for none and ignores any statement a device sends anyway, rather than refusing devices that send one.
- **Verification in Rust ops, not JavaScript or Web Crypto.** 45's inventory row said Web Crypto, but the server runtime has none ([33, "Implementation"](33-crypto.md#implementation)); CBOR, the authenticator data and the COSE keys are parsed once, in Rust, next to `ring`'s signature checks, so no JavaScript CBOR or crypto library is added on either side, and the parser is unit-tested where the rest of the server's crypto is.
- **The challenge in the session row** reuses the OAuth state's storage, lifetime and single-use rule, so passkeys add no table, no migration of `k_session` and no signing secret, and a challenge can't outlive or leave its session.
- **The user handle in the challenge** means `registrationOptions` and `verifyRegistration` need nothing remembered between them besides the challenge itself, and the browser can't swap the handle, because the challenge it would have to change is the stored one.
- **The credential in the app's entity** follows how Kobra treats accounts: the app owns its account model, and the stdlib supplies the mechanism (as `Password` supplies hashing without an account table).
- **`@UserAction` on both ceremonies** makes Safari's rule the compiler's rule, and pushes the challenge fetch to before the click, which is also what keeps the sheet quick.

## Alternatives rejected

- **A hosted passkey service, or the FIDO Metadata Service:** a third-party account or trust list for a check that adds nothing here.
- **A stdlib `k_passkey` table and accounts:** it would decide the account model for the app, and duplicate what an entity does better (typed ids, migrations, relations).
- **Parsing CBOR in JavaScript:** a second parser to keep correct, in the bundle that ships to the browser for the test double, or on the server outside Rust's tests and fuzzing.
- **Returning a result with flags for the app to check** (`userVerified: Bool`, `counterOk: Bool`): a passkey "verified" with a cloned counter or no UV is an easy mistake; all-or-nothing makes the safe outcome the only one.
- **`EcdsaP256PublicKey`, `Ed25519PublicKey` and `RsaPublicKey` in the credential:** a sum type of three key types, three column types, and no app use beyond the verifier.
- **Fetching options inside the click:** fails in Safari, and K4030 would refuse it anyway.

## Open questions

- **Autofill (conditional mediation):** `navigator.credentials.get({mediation: "conditional"})` offers passkeys in the email field's autofill, and needs `autocomplete="username webauthn"` on the field, which Kobra's views have no modifier for yet. A `Passkeys.autofill(_:)` that isn't `@UserAction` (choosing from the autofill list is the activation) fits the API above once the field can say so.
- **Adding a passkey to an account, listing and removing them,** and the signal API (`PublicKeyCredential.signalUnknownCredential`) for a removed one: app code over the entity, which the notes example doesn't show yet.
- **An RP id other than the origin's host** (a parent domain shared by a workspace's apps), which would need a config value and related origins.
- **Transports and authenticator names** (`getTransports()`, AAGUID lookups) for nicer `allowCredentials` and account pages.
- **Several challenges per session,** if a page ever needs a sign-in and a registration ceremony open at once.
