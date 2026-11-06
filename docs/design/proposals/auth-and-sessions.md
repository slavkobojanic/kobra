# Proposal: Authentication, Sessions and Guard Rejections (`k.auth`)

> **Status: proposal.** Not yet accepted into the design. First used by `examples/notes`, and consistent with how `examples/shop` uses `k.auth`.

## Problem

[20-standard-library.md](../20-standard-library.md) says `k.auth` provides "sessions, OAuth providers, passkeys, the `Authenticated` and `Role` guards", and the README lists the auth and session model as not yet covered. The design uses `Session` and `session.user.id` ([11-protocols-and-rpc.md](../11-protocols-and-rpc.md)) and `requires Authenticated` on pages and rpcs ([12-routing.md](../12-routing.md)) without saying:

- what `Session` holds, how a sign-in starts one, and what the client can read;
- what a page or rpc caller experiences when a guard refuses;
- how passwords are hashed and checked;
- how tests act as a signed-in user.

## Proposal

### `Session`

`Session` is a stdlib service in `k.auth`. Like any service, it is injected; it is `@Scope(.request)` on the server and `@Scope(.app)` in the browser.

| Member | Where | Meaning |
|---|---|---|
| `user: SessionUser?` | both | Who is signed in, or `nil`. Reactive. |
| `signIn(userId: String, roles: Set<R> = Set([])) async` | server only | Starts a new session for `userId`, replacing any current one. `R` is the app's `AuthRole` enum. |
| `signOut() async` | server only | Ends the current session. Does nothing for a guest. |

`SessionUser` is a struct with `id: String` and `roles: Set<R>`. The id is whatever the app uses to identify a user, usually an entity's primary key written as text (`"{account.id}"`).

- **Calling a server-only member from client code is a compile error**, through the existing server/client boundary ([10-services.md](../10-services.md)).
- **In the browser, `user` is a read-only mirror.** Server rendering serializes it into the page, and every rpc response that changes the session (`signIn`, `signOut`, expiry) updates it. Client services read it to show account state, and the router reads it to evaluate page guards during client-side navigation without a round trip.
- **Storage:** the session cookie holds only a random, unguessable session id (`HttpOnly`, `SameSite=Lax`, and `Secure` everywhere except `localhost`). Session data lives on the server in a table `k.auth` manages itself (`k_session`, outside the app's `schema.lock`), so `signOut` revokes a session immediately, and no signing secret has to be configured.
- **`signIn` always issues a new session id**, which prevents session fixation.
- Sessions last 30 days and slide on use. Changing the lifetime is left open (see below).

### Narrowing under `Authenticated`

Inside code guarded by `Authenticated`, **`session.user` has type `SessionUser`, not `SessionUser?`**:

- the body of an rpc that `requires Authenticated`, or of every rpc of a protocol that does (`protocol NotesProtocol requires Authenticated`);
- a page that `requires Authenticated`, including its `@Load` initializers.

This is what lets [11-protocols-and-rpc.md](../11-protocols-and-rpc.md) write `session.user.id` with no unwrapping. The guard has already run, so the narrowing is sound. Private helpers of such a service are narrowed too when every caller is guarded; otherwise they see `SessionUser?`.

### Guards

| Guard | Refuses when |
|---|---|
| `Authenticated` | nobody is signed in |
| `Role(.admin)` | the user lacks the role. The argument is a case of the app's single `enum … implements AuthRole`. |
| `RateLimit(10.perMinute)` | the client (session, or IP address for guests) exceeded the limit for this rpc |

All three live in `k.auth`.

### What a refused page does

- **`Authenticated` refusing a page redirects to the app's sign-in page,** named once with `@SignInPage(SignIn)` on the `app` ([app-structure.md](app-structure.md)). When the sign-in page declares `@Param let returnTo: Page`, the redirect fills it with the page that was refused, so the person comes back after signing in.
- `Role` refusing a page responds `403 Forbidden`, rendered by the nearest `ErrorBoundary`.
- With no `@SignInPage`, a page that `requires Authenticated` is a compile error, because there would be nowhere to send the visitor.

### What a refused rpc throws

A guard runs before the handler, so its refusal is part of the rpc's contract. It is a **typed error, like any other**:

```k
public enum GuardRejection implements Error {
    case unauthenticated
    case forbidden
    case rateLimited(retryAfter: Duration)
}
```

- **An rpc with a `requires` clause (its own or its protocol's) must declare a `throws` type with exactly one case whose only payload is `GuardRejection`.** By convention it is `case rejected(GuardRejection)`. The runtime throws that case on the client when a guard refuses, so `catch` stays exhaustive and every caller decides what a refusal means (usually `Router.navigate(to: .signIn.returnTo(…))`).
- A guarded rpc without such a case is a compile error, with a quick fix that adds `case rejected(GuardRejection)` to its error enum, or declares a new one.
- An rpc that can't otherwise fail still declares a small error enum holding only `rejected`. The cost is one enum per module; the benefit is that an expired session is never an unhandled surprise.

### Passwords (`k.crypto`)

```k
let hash = await Password.hash(password: request.password)            // Argon2id, as a PHC string
let matches = await Password.verify(password: typed, hash: stored)     // constant time
```

- Both are server-only and `async`, because hashing is deliberately slow.
- **`Password.unusableHash`** is a valid hash that no password matches. Verifying against it when no account has the email keeps sign-in timing the same whether or not the email is registered.
- The plain-text password is a `String` only for the duration of the request. Logging a field named `password`, or one whose type is `Secret<…>`, is a compile-time warning.

### Tests

- **Every test gets a fresh container ([testing-support.md](testing-support.md)), so a fresh, signed-out `Session`.** Calls made in one test share that session, exactly as several requests from one browser would.
- **Tests run on the server side of the boundary,** so a test may call `session.signIn(userId:roles:)` directly to act as a user without creating an account. Tests of sign-in itself go through the real rpcs instead.
- Guards run in tests exactly as in production, so `expect throws AccessError.rejected(.unauthenticated) { … }` checks that an rpc is protected.

## Why

- A request-scoped server `Session` with a read-only client mirror keeps one name for "who is signed in" on both sides, while the server/client boundary keeps writes on the server.
- Narrowing turns the guard into a type fact, so handlers contain no defensive `guard let` that could never fail.
- Typed rejections follow [09-errors.md](../09-errors.md): every point where a call can fail is visible in its signature and handled exhaustively. A runtime-only redirect would be invisible to the code that has to cope with it, for example an unsaved draft.
- Server-side session storage avoids a signing secret, which removes the most common auth misconfiguration, and makes sign-out mean what it says.

## Open questions

- Configuring the session lifetime and cookie name: an annotation on the `app`, such as `@SessionLifetime(30.days)`, or a stdlib `SessionConfig` ([config-and-secrets.md](config-and-secrets.md)).
- OAuth providers and passkeys build on `signIn(userId:roles:)` but need their own callback pages (`at "/auth/github/callback"`) and provider interfaces.
- Whether `RateLimit` should key on the account rather than the session for signed-in users.
