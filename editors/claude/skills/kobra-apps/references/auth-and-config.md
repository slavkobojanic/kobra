# Sign-in, sessions, guards, configuration and secrets

Sources: `book/apps/auth.md` and `config.md`, and `design/25-config-and-secrets.md`,
`26-auth-and-sessions.md` and `51-passkeys.md`.

## Identity

The app declares exactly one `AuthUserId` type, which is then the type of `SessionUser.id`:

```k
import k.auth.AuthUserId
import k.crypto.Uuid

/// The id of an account, and of the signed-in user.
public struct AccountId implements AuthUserId {
    let value: Uuid
}
```

Roles are optional: one `enum StaffRole implements AuthRole { case admin }`, then
`sessions.signIn(userId: id, roles: Set([StaffRole.admin]))`, and `requires Role(.admin)`.

## `Session` and `SessionManager`

- `Session` (`k.auth`, both sides): `session.user: SessionUser?`, which is reactive. In the browser
  it's a read-only mirror.
- `SessionManager` (`k.auth`, **server-only**): `signIn(userId:)`, `signOut()`, `endAll(userId:)`,
  `issueState()` and `consumeState(_:)`. It's injected into server-side services only.
- Sessions live in the database (`k_session`), and the cookie holds only a random id.

```k
import k.auth.SessionManager
import k.crypto.Password
import k.rpc.RpcOnlyError

/// The server side of `AuthProtocol`.
public service AuthApi implements AuthProtocol {
    @Inject let accounts: AccountRepository
    @Inject let sessions: SessionManager

    rpc signIn(credentials: Credentials) async throws SignInError {
        let account = await accounts.byEmail(email: "{credentials.email}")

        // Verify against a hash even when no account has this email, so the
        // response time doesn't reveal which emails are registered.
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
```

The protocol it implements puts the rate limit on the contract:

```k
import k.auth.RateLimit
import k.rpc.RpcOnlyError

/// Signing in and out.
public protocol AuthProtocol {
    rpc signIn(credentials: Credentials) throws SignInError requires RateLimit(10.perMinute)

    rpc signOut() throws RpcOnlyError
}
```

```k
import k.rpc.RpcFailure

/// Why signing in failed.
public enum SignInError implements Error {
    case invalidCredentials
    case rpcFailure(RpcFailure)
}
```

- `Password.hash(password:)`, `Password.verify(password:hash:)` and `Password.unusableHash` come from
  `k.crypto` (Argon2id, server-only, async). `k hash-password` hashes a password from stdin.
- **Guards:** `Authenticated`, `Role(.x)` and `RateLimit(10.perMinute)`, all imported from `k.auth`.
  They go in a `page` header, a `protocol` header or on an rpc of a protocol. They're never written
  on handlers (K5507), layouts (K5014) or endpoint pages (except `RateLimit`).
- **Narrowing:** `session.user` is non-optional only in rpcs guarded by `Authenticated` and in pages
  that `requires Authenticated` (their `@Load`s included). Private helpers aren't narrowed, so pass
  the `SessionUser` explicitly.
- Sign-in form: see the `SignIn` page pattern in [services-and-rpc.md](services-and-rpc.md). The
  sign-in page takes `@Param let returnTo: Page = .home` and is named with `@SignInPage(.signIn)`.
- Passkeys (`Passkeys` in the browser, `PasskeyVerifier` on the server) are WebAuthn with no third
  party (`read_doc book/apps/auth.md`). OAuth providers are deferred by the design.
- CSRF protection is always on: RPC accepts only JSON plus `Connect-Protocol-Version`, and requests
  that carry the cookie are checked against `Origin`.

## Configuration: `config` declarations

```k
import k.validation.Matches

/// Settings for verifying payment webhooks.
public config WebhookConfig {
    /// The endpoint's signing secret.
    @Matches("^whsec_[A-Za-z0-9]+$") @Env("WEBHOOK_SECRET") let signingSecret: Secret<String>

    /// How far a signature's timestamp may be from now.
    @Env("WEBHOOK_TOLERANCE") let tolerance: Duration = 5.minutes
}
```

```k
/// Checks the shared secret webhook senders put in a header.
public service WebhookVerifier {
    @Inject let config: WebhookConfig

    func isAuthentic(token: String) -> Bool {
        return config.signingSecret.matches(token)
    }
}
```

- Every field names its variable with `@Env("NAME")`, because names are never derived. A field is
  `let`. A default makes it optional, and `T?` is optional with `nil`.
- The field types are `String`, `Int`, `Double`, `Decimal`, `Bool`, `Duration` (`30s`, `5m`), `Url`,
  `Uuid`, `EmailAddress`, payload-free enums, `List<T>` (comma-separated) and `Secret<T>`.
- Configs are **server-only** and are data: no functions, `@Inject` or state. Inject them into
  server-side services and repositories.
- Values come from `NAME_FILE` (for secrets, the Docker and Kubernetes way), then `NAME`, then the
  default. `dist/app` validates every reachable config at startup, and exits with status 78 listing
  every problem. `dist/app config` prints the variable table.
- In development, values come from the process environment, then `.env`, then `[dev.env]` and
  `[dev.secrets]` in `k.toml`, then the defaults (SQLite `DATABASE_URL`, and a local mail catcher).
  Tests never read the environment.
- **Which variables are required follows from the `use` rules**: a config injected only by a
  production-only implementation isn't needed in `k dev`.
- Standard configs: `PORT`, `K_PUBLIC_ORIGIN` (set it in every deployment), `DATABASE_URL`,
  `DATABASE_PASSWORD`, `K_JOB_CONCURRENCY`, `K_SESSION_IDLE`, `K_SESSION_MAX_AGE` and the
  `SMTP_*`/`MAIL_FROM` variables.

## `Secret<T>`

`Secret(value)`, `secret.reveal()` (greppable on purpose) and `secret.matches(candidate)`, which is
constant-time. It isn't `Codable`, `Equatable` or `Hashable`, it interpolates as `Secret(•••)`, and
comparing a revealed value with `==` is K2001. HTTP requests take it directly with
`.bearerToken(config.apiKey)`.

## Configs in tests

Bind a config in a test group with `use WebhookConfig.sample for WebhookConfig`. Define `sample` in a
test-only module:

```k
public extend WebhookConfig {
    static let sample = WebhookConfig(signingSecret: Secret("whsec_test"))
}
```

A test graph that reaches an unbound app config is K7503.
