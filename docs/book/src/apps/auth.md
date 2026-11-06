# Authentication and sessions

`k.auth` provides sessions and guards. **Who is signed in** is the `Session` service, readable on both
sides. **Signing in and out** is the server-only `SessionManager` service. Sessions live in the app's
database; the cookie holds only a random id.

```k
// src/accounts/AccountId.k

/// The id of an account, and of the signed-in user.
public struct AccountId implements AuthUserId {
    let value: Uuid
}
```

```k
// src/accounts/AuthApi.k
public service AuthApi implements AuthProtocol {
    @Inject let accounts: AccountRepository
    @Inject let sessions: SessionManager

    rpc signIn(credentials: Credentials) async throws SignInError {
        let account = await accounts.byEmail(email: credentials.email)

        // Verify even when no account has this email, so timing doesn't reveal which are registered.
        let hash = if let account { account.passwordHash } else { Password.unusableHash }
        let matches = await Password.verify(password: credentials.password, hash: hash)

        guard let account, matches else {
            throw SignInError.invalidCredentials
        }

        await sessions.signIn(userId: account.id)
    }
}
```

```k
// src/accounts/AuthProtocol.k
public protocol AuthProtocol {
    rpc signUp(request: SignUpRequest) throws SignUpError requires RateLimit(10.perMinute)

    rpc signIn(credentials: Credentials) throws SignInError requires RateLimit(10.perMinute)
}
```

## `Session` and `SessionManager`

The server/client boundary is drawn per declaration, so the two faces of a session are two services.

**`Session`** (`@Scope(.app)`: one per request on the server, one per app in the browser) is client-safe:

| Member | Meaning |
|---|---|
| `user: SessionUser?` | who is signed in, or `nil` for a guest. Reactive. |

**`SessionManager`** is server-only, and injectable only into server-side services:

| Member | Meaning |
|---|---|
| `signIn(userId:roles:) async` | starts a new session for the user, replacing any current one. `roles` exists only when the app declares an `AuthRole` enum. |
| `signOut() async` | ends the current session. Does nothing for a guest. |
| `endAll(userId:) async` | ends every session of a user, on every device; call it when roles or the password change |
| `issueState() async -> String` | a random value bound to the current session, for an OAuth `state` parameter |
| `consumeState(_ state: String) async -> Bool` | `true` once, if `state` was issued to this session |

- **In the browser, `Session` is a read-only mirror.** Server rendering serializes `user` into the page,
  and every rpc response that changes the session updates it. Client services read it to show account
  state, and the router reads it to evaluate page guards during client-side navigation without a round
  trip.
- **`signIn` always issues a new session id,** which prevents session fixation.

### Typed user ids and roles

- **The app declares exactly one type implementing `AuthUserId`**, usually the id type of its account
  entity, and `SessionUser.id` has that type. Ids stay typed, never text built by interpolation.
- **An app with roles declares exactly one `enum … implements AuthRole`**, and `SessionUser.roles` is a
  `Set` of it. With no `AuthRole` enum, `roles` doesn't exist and `Role(…)` is a compile error.
- Like `Page`, `SessionUser` is generated for the app and needs no import.

```k
public struct StaffId implements AuthUserId {
    let email: EmailAddress
}

public enum StaffRole implements AuthRole {
    case admin
}

// in a server-side service
await sessions.signIn(userId: StaffId(email: config.email), roles: Set([StaffRole.admin]))
```

## Narrowing under `Authenticated`

**`session.user` has type `SessionUser`, not `SessionUser?`, only in bodies that are syntactically
guarded:**

- the body of an rpc whose own `requires` clause, or whose protocol's, includes `Authenticated`;
- a page that `requires Authenticated`, including its `@Load` initializers.

```k
public service NotesApi implements NotesProtocol {        // NotesProtocol requires Authenticated
    @Inject let session: Session
    @Inject let notes: NoteRepository

    rpc get(id: Uuid) async throws LookupError -> NoteSummary {
        guard let note = await notes.find(id: id, ownerId: session.user.id) else {
            throw LookupError.notFound
        }

        return summarize(note: note)
    }
}
```

Nothing else is narrowed, including private helpers called only from guarded rpcs. A helper takes the
user explicitly, so its types never depend on who calls it:

```k
private func ownerOf(user: SessionUser) -> AccountId { … }
```

## Guards

| Guard | Refuses when |
|---|---|
| `Authenticated` | nobody is signed in |
| `Role(.admin)` | the user lacks the role; the argument is a case of the app's `AuthRole` enum |
| `RateLimit(10.perMinute)` | the client exceeded the limit for this rpc or page; signed-in clients are counted per session, guests per client address |

Rate-limit counters live in the app's database (a standard table, `k_rate_limit`), so a limit holds
across every replica.

### What a refused page does

- **`Authenticated` refusing a page redirects with `303 See Other` to the app's sign-in page,** named
  once with `@SignInPage(.signIn)` on the `app`. When the sign-in page declares `@Param let returnTo:
  Page`, the redirect fills it with the refused page, so the person comes back after signing in, and the
  parameter can't send them to another site.
- `Role` refusing a page responds `403 Forbidden`, rendered by the nearest `ErrorBoundary`.
- With no `@SignInPage`, a page that `requires Authenticated` is a compile error.

### What a refused rpc throws

A guard runs before the handler, so its refusal is part of the rpc's contract. It arrives as
**`rpcFailure(.rejected(GuardRejection))`**, the fixed case every rpc's error enum has:

```k
public enum GuardRejection implements Error {
    case unauthenticated
    case forbidden
    case rateLimited(retryAfter: Duration)
}
```

Every caller decides what a refusal means, so an expired session is never an unhandled surprise:

```k
do {
    try await auth.signIn(credentials: credentials.value)

    Router.navigate(to: returnTo)
} catch .invalidCredentials {
    problem = "That email and password don't match."
} catch .rpcFailure(.rejected(.rateLimited)) {
    problem = "Too many attempts. Wait a minute and try again."
} catch .rpcFailure {
    problem = "We couldn't sign you in. Try again."
}
```

## Signing up without races

A unique email is checked by the database, not by a lookup first. The account entity marks the field
`@Unique`, so `insert` throws a typed `UniqueViolation` that the handler maps to its own error:

```k
rpc signUp(request: SignUpRequest) async throws SignUpError {
    let account = Account(
        id: AccountId(value: Uuid.random()),
        email: request.email,
        passwordHash: await Password.hash(password: request.password)
    )

    do {
        try await accounts.insert(account)
    } catch .email {
        throw SignUpError.emailTaken
    }

    await sessions.signIn(userId: account.id)
}
```

## Lifetime and revocation

- **Sessions have an idle timeout and an absolute lifetime.** The idle timeout (7 days by default) slides
  on use; the absolute lifetime (30 days from `signIn`) doesn't. Both come from the standard
  `SessionConfig` (`K_SESSION_IDLE`, `K_SESSION_MAX_AGE`).
- **Session data lives on the server,** in a standard table (`k_session`, outside `schema.lock`). So
  `signOut` and `endAll` revoke immediately, and no signing secret has to be configured.
- **Roles are read from the session,** captured at `signIn`. A role change takes effect when the user's
  sessions are ended with `endAll(userId:)`.
- **Ending a session closes its streams,** whether by sign-out, expiry or `endAll`
  ([Server streaming](streaming.md)).

## Cookies and caching

- **The cookie holds only a random, unguessable session id.** It is `HttpOnly`, `SameSite=Lax`, `Path=/`.
- **`Secure` follows the scheme of `K_PUBLIC_ORIGIN`,** not of the connection, so it is right behind a
  proxy that terminates TLS. With an `https` origin the cookie is named `__Host-k_session`; with an `http`
  origin (local development) it is `k_session`.
- **Every rpc response, and every response whose rendering read `Session`, carries `Cache-Control:
  private, no-store`,** because server-rendered HTML embeds the session mirror.

## Request forgery

K's defences against cross-site request forgery are on by default and can't be turned off:

- **RPC routes accept only `application/json` and `application/proto`, and require the
  `Connect-Protocol-Version` header.** Anything else is refused with `415` before guards run. A
  cross-site `<form>` can't send either, and a cross-site `fetch` that tries needs a CORS preflight,
  which K never grants for rpc routes.
- **Every request that carries the session cookie and isn't `GET` or `HEAD`** must have an `Origin`
  header matching `K_PUBLIC_ORIGIN`, or `Sec-Fetch-Site: same-origin`, or it is refused with `403`.
- **A `GET` or `HEAD` handler on an endpoint page that uses the session must not change state.** Reaching
  `SessionManager.signIn`, `signOut` or a repository write from one is a compile warning, except in an
  OAuth callback that returns early unless `consumeState` succeeded.
- **Endpoint pages that don't use the session receive no cookie and need no check,** which is what
  webhooks want ([Endpoint pages](endpoints.md#sessions)).

## OAuth callbacks

An OAuth provider sends the user back to a fixed URL, so the callback is an
[endpoint page](endpoints.md) that uses the session. Before redirecting to the provider, the app calls
`issueState()` and passes the value as `state`. The callback page injects `Session`, and a server-side
service it calls checks `consumeState(state)` before calling `signIn`. It ends with
`HttpResponse.redirect(to: .home)`.

OAuth providers and passkeys themselves are deferred by the design.

## Passwords

`Password.hash(password:)` and `Password.verify(password:hash:)` (Argon2id, server-only, `async`) and
`Password.unusableHash` are in `k.crypto` ([Bytes and cryptography](crypto.md#passwords)). Logging a field
named `password`, or one whose type is `Secret<…>`, is a compile-time warning.

## Tests

- **Every test gets a fresh, signed-out session.** Calls made in one test share it, exactly as several
  requests from one browser would.
- A test acts as a user with `@Inject let sessions: SessionManager` and
  `await sessions.signIn(userId: AccountId.ada)`, without creating an account. Tests of sign-in itself go
  through the real rpcs.
- Guards run in tests exactly as in production:

```k
test NotesApi {
    @Inject let notes: NotesProtocol

    test "notes are private" {
        expect throws AccessError.rpcFailure(.rejected(.unauthenticated)) {
            try await notes.list()
        }
    }
}
```
