# Authentication and Sessions

## Decision

`k.auth` provides sessions and guards. **Who is signed in** is the `Session` service, readable on both sides. **Signing in and out** is the server-only `SessionManager` service. Sessions live in the app's database; the cookie holds only a random id.

```k
// accounts/AccountId.k
/// The id of an account, and of the signed-in user.
public struct AccountId implements AuthUserId {
    let value: Uuid
}

// accounts/AuthApi.k
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

// notes/NotesApi.k
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

## `Session` and `SessionManager`

The boundary is drawn per declaration ([10-services.md](10-services.md#serverclient-boundary)), so the two faces of a session are two services.

**`Session`** (`k.auth`, `@Scope(.app)`: one per request on the server, one per app in the browser) is client-safe:

| Member | Meaning |
|---|---|
| `user: SessionUser?` | who is signed in, or `nil` for a guest. Reactive. |

**`SessionManager`** (`k.auth`) is server-only and injectable only into server-side services:

| Member | Meaning |
|---|---|
| `signIn(userId:roles:) async` | starts a new session for the user, replacing any current one. `roles` exists only when the app declares an `AuthRole` enum (below). |
| `signOut() async` | ends the current session. Does nothing for a guest. |
| `endAll(userId:) async` | ends every session of a user, on every device. Apps call it when a user's roles or password change. |
| `issueState() async -> String` | a random value bound to the current session (creating a guest session if needed), for an OAuth `state` parameter |
| `consumeState(_ state: String) async -> Bool` | `true` once, if `state` was issued to this session; it can't be used again |

- **In the browser, `Session` is a read-only mirror.** Server rendering serializes `user` (with its roles, by case name) into the page, and every rpc response that changes the session (sign-in, sign-out, expiry) updates it. A session that ended without this browser's help (signed out in another tab, `endAll` from another device, expired) counts as a change: the next rpc response that arrives with its dead cookie carries the guest mirror and clears the cookie. Client services read the mirror to show account state, and the router reads it to evaluate page guards during client-side navigation without a round trip ([below](#in-the-browser)).
- **`signIn` always issues a new session id,** which prevents session fixation.

### `SessionUser` is generated

- **The app declares exactly one type implementing `AuthUserId`** (usually the id type of its account entity, as `AccountId` above), and `SessionUser.id` has that type. Ids stay typed, never text built by interpolation. Using `SessionManager` with none, or with several, is a compile error with a quick fix.
- **Roles:** an app with roles declares exactly one `enum … implements AuthRole` (`enum StaffRole implements AuthRole { case admin }`), and `SessionUser.roles` is a `Set` of it. With no `AuthRole` enum, `roles` doesn't exist and `Role(…)` is a compile error.
- Like `Page`, `SessionUser` is generated for the app and needs no import.

## Narrowing under `Authenticated`

**`session.user` has type `SessionUser`, not `SessionUser?`, only in bodies that are syntactically guarded:**

- the body of an rpc whose own `requires` clause in the protocol, or the protocol's, includes `Authenticated`; when the service implements several protocols that declare the rpc, one handler serves them all, so every one of them must guard it;
- a page that `requires Authenticated`, including its `@Load` initializers.

This is what lets handlers write `session.user.id` with no unwrapping; the guard has already run, so the narrowing is sound. **Nothing else is narrowed,** including bodies behind only `RateLimit` or `Role`, and private helpers called only from guarded rpcs. A helper takes the user explicitly, so its types never depend on who calls it:

```k
private func ownerOf(user: SessionUser) -> AccountId { … }
```

## Guards

| Guard | Refuses when |
|---|---|
| `Authenticated` | nobody is signed in |
| `Role(.admin)` | the user lacks the role. The argument is a case of the app's `AuthRole` enum. |
| `RateLimit(10.perMinute)` | the client exceeded the limit for this rpc or page. Signed-in clients are counted per session, guests per client address ([27-endpoint-pages.md](27-endpoint-pages.md#the-incoming-request)). |

- **Rate-limit counters live in the app's database** (a stdlib table `k_rate_limit`, like `k_session`), so a limit holds across every replica instead of multiplying by the replica count.

### Where guards are written

A guard is written only where the server checks it, so a declaration never looks protected without being protected:

- **`page … requires`**, checked on every request for the page (and by the router in the browser, [below](#in-the-browser)).
- **`protocol … requires`** and **`rpc … requires` in a protocol**, checked before the handler runs, for in-process calls too ([11-protocols-and-rpc.md](11-protocols-and-rpc.md#rules)).

Anywhere else a clause is an error, never silently accepted:

- **On a service's handler** (`rpc` with a body), or on an `rpc` no protocol declares (which is itself `K5508`, [11-protocols-and-rpc.md](11-protocols-and-rpc.md#rules)): `K5507`. The handler serves the protocol's rpc, and the protocol's guards are what runs. The quick fix moves the clause to the protocol's rpc, removing the imports only the clause used; for a protocol from a library, which the app can't change, the message says the library declares its guards and the handler must check the caller itself. A clause on the handler never narrows `session.user` either ([above](#narrowing-under-authenticated)).
- **On a layout:** `K5014`, with a help to put the clause on each page that renders in it ([12-routing.md](12-routing.md)).
- **After a function's signature, or in a component or service header:** a syntax error, as the grammar has no clause there.

### What a refused page does

- **`Authenticated` refusing a page redirects with `303 See Other` to the app's sign-in page,** named once with `@SignInPage(.signIn)` on the `app` ([23-app-structure.md](23-app-structure.md)). When the sign-in page declares `@Param let returnTo: Page`, the redirect fills it with the refused page, encoded as a path within the app, so the person comes back after signing in and the parameter can't send them to another site.
- `Role` refusing a page responds `403 Forbidden`, rendered by the nearest `ErrorBoundary`.
- With no `@SignInPage`, a page that `requires Authenticated` is a compile error, because there would be nowhere to send the visitor.

#### In the browser

The server checks a page's guards on every request for it. The router checks the ones it can against the mirror, so a page it can't show is never rendered in the browser:

- **The client route table carries each page's `Authenticated` and `Role` guards.** `RateLimit` stays with the server, which counts the requests it sees.
- **A navigation to a page the mirror refuses is a full page load of its URL,** instead of a client-side render. A link, `Router.navigate(to:)` and Back/Forward all go this way. The server then answers as it does for any request: the `303` to the sign-in page with `returnTo`, or `403`. The browser never has to build the sign-in URL itself, so the return URL is exactly the server's. `Authenticated` is refused when the mirror has no user. `Role` is refused when the mirror's roles don't include the case. A mirror that doesn't list roles can't tell, so the page is loaded and the server decides.
- **Losing the user while a guarded page is shown leaves it the same way.** This covers `signOut`, `endAll`, an expired session, or a lost role. The router checks each new mirror an rpc response brings before setting it. The server sends the mirror with every response that finds the session changed, an opened stream's head included, and with every rpc an `Authenticated` or `Role` guard refuses: the server can't know what the browser's copy says, and a refusal is when that copy is likeliest to be stale (a role taken away has no response of its own, and a cookie cleared by an earlier non-rpc response leaves nothing to find). A page the new mirror refuses is disposed first, so its narrowed body never runs with `session.user` absent. Then the router does a full page load of the URL being shown, or of the one being navigated to. After that the router renders nothing more, so an app's own `Router.navigate(to: .signIn)` after signing out is ignored, and the load decides where the person lands.
- **Work the page started stops with it.** A handler, effect or load of the page that is still awaiting when the session ends is owned by the page ([03-reactivity.md](03-reactivity.md#async-work-is-owned)). It stops at its `await` even when its own rpc answered just before the session-ending response arrived, so the code after the `await`, which reads `session.user` without a `nil` check, never runs once the user is gone. Its pending requests are aborted. Nor does anything else the page left: its nodes, still on screen until the full page load replaces them, answer no clicks, and the listeners it gave an app-wide service and the timers it started run nothing ([03-reactivity.md](03-reactivity.md#what-a-component-leaves-behind)).
- **The first page of a document isn't checked again.** The server checked its guards when it answered the request, and a recheck could only loop.
- **A client-rendered app with a server works the same way** ([23-app-structure.md](23-app-structure.md#rendering-mode)). The server checks a page's guards before it answers the page's URL with the shell, so a guest asking for a guarded page gets the same `303` or `403` as in a server-rendered app. The shell embeds the session mirror in `k-data` (`{"v":1,"session":{…},"c":{}}`, with no render instant, since nothing was rendered), so `Session` and the router start from who is signed in, and the shell is `private, no-store` when someone is. A static-only app has no server and so no sessions, and its pages can't declare guards that need one.

### What a refused rpc throws

A guard runs before the handler, so its refusal is part of the rpc's contract. It arrives as **`rpcFailure(.rejected(GuardRejection))`**, the fixed case every rpc's error enum has ([28-rpc-wire-format.md](28-rpc-wire-format.md#failures-outside-the-handler)):

```k
public enum GuardRejection implements Error {
    case unauthenticated
    case forbidden
    case rateLimited(retryAfter: Duration)
}
```

Every caller decides what a refusal means, usually `Router.navigate(to: .signIn.returnTo(…))`, so an expired session is never an unhandled surprise.

## Lifetime and revocation

- **Sessions have an idle timeout and an absolute lifetime.** The idle timeout (default 7 days) slides on use; the absolute lifetime (default 30 days from `signIn`) doesn't. Both are set through the stdlib `SessionConfig` (`K_SESSION_IDLE`, `K_SESSION_MAX_AGE`; [25-config-and-secrets.md](25-config-and-secrets.md#stdlib-configs)).
- **Session data lives on the server,** in a table `k.auth` manages itself (`k_session`, outside `schema.lock`). So `signOut` and `endAll` revoke immediately, and no signing secret has to be configured.
- **Roles are read from the session,** captured at `signIn`. A role change takes effect when the user's sessions are ended with `endAll(userId:)`.
- **Ending a session closes its streams.** The runtime closes every server stream opened under a session when that session ends, by sign-out, expiry or `endAll` ([29-server-streaming.md](29-server-streaming.md)).

## Cookies and caching

- **The cookie holds only a random, unguessable session id.** It is `HttpOnly`, `SameSite=Lax`, `Path=/`.
- **`Secure` follows the scheme of `K_PUBLIC_ORIGIN`,** not of the connection, so it is right behind a proxy that terminates TLS. With an `https` origin, the cookie is named **`__Host-k_session`**, which browsers refuse to accept from a subdomain or without `Secure`; with an `http` origin (local development) it is `k_session`.
- **Every rpc response, every response whose rendering read `Session`, and every document rendered for a signed-in user carries `Cache-Control: private, no-store`,** because server-rendered HTML, and a client-rendered app's shell, embeds the session mirror in `k-data`, whether or not the page reads it. A shared cache never serves one user's page, or their id and roles, to another. A guest's document carries the guest mirror and may be cached; a browser handed one while signed in only defers more to the server, whose guards decide.

## Request forgery

K's defences against cross-site request forgery are on by default and can't be turned off:

- **RPC routes accept only `application/json` and `application/proto`, and require the `Connect-Protocol-Version` header.** Anything else is refused with `415` before guards run. A cross-site `<form>` can't send either, and a cross-site `fetch` that tries needs a CORS preflight, which K never grants for rpc routes.
- **Every request that carries the session cookie and isn't `GET` or `HEAD`** must have an `Origin` header matching `K_PUBLIC_ORIGIN`, or `Sec-Fetch-Site: same-origin`, or it is refused with `403`. This covers rpcs and endpoint pages that use the session, and it also stops same-site attackers (a sibling subdomain), which `SameSite=Lax` alone doesn't.
- **A `GET` or `HEAD` handler on an endpoint page that uses the session must not change state:** reaching `SessionManager.signIn`, `signOut` or a repository write from one is a compile warning. The exception is an OAuth callback that returns early unless `consumeState` succeeded, which the compiler checks in the same function.
- **Endpoint pages that don't use the session receive no cookie and need no check,** which is what webhooks want ([27-endpoint-pages.md](27-endpoint-pages.md#sessions)).

## OAuth callbacks

An OAuth provider sends the user back to a fixed URL, so the callback is an endpoint page ([27-endpoint-pages.md](27-endpoint-pages.md)) that uses the session:

- Before redirecting to the provider, the app calls `issueState()` and passes the value as `state`.
- The callback page injects `Session`, and a server-side service it calls checks `consumeState(state)` before calling `signIn`. The state lives in the server-side session row, so no hand-written cookie is needed and login CSRF is prevented.
- The callback ends with `HttpResponse.redirect(to: .home)`.

## Passwords

`Password.hash(password:)` and `Password.verify(password:hash:)` (Argon2id, server-only, `async`) and `Password.unusableHash` are in `k.crypto` ([33-crypto.md](33-crypto.md#passwords)). Logging a field named `password`, or one whose type is `Secret<…>`, is a compile-time warning.

## Tests

- **Every test gets a fresh, signed-out session** ([21-testing.md](21-testing.md#which-side-a-test-runs-on)). Calls made in one test share it, exactly as several requests from one browser would.
- A test acts as a user with `@Inject let sessions: SessionManager` and `await sessions.signIn(userId: …)`, without creating an account. Tests of sign-in itself go through the real rpcs.
- Guards run in tests exactly as in production: `expect throws AccessError.rpcFailure(.rejected(.unauthenticated)) { … }` checks that an rpc is protected.

## Why

- Two services keep the server/client boundary per declaration: anything may read who is signed in, and only server code can change it.
- Narrowing only syntactically guarded bodies turns the guard into a type fact without making a helper's types depend on the whole call graph.
- Typed rejections follow [09-errors.md](09-errors.md): every point where a call can fail is visible in its signature and handled exhaustively.
- Server-side session storage avoids a signing secret, which removes the most common auth misconfiguration, and makes sign-out and `endAll` mean what they say.
- The client-rendered shell carries the mirror rather than leaving the browser to ask for it, so a client-rendered app's guards, its first render of `Session` and its caching rules are the server-rendered app's, with no extra request before the first page. Without it, the router would see a guest until the first rpc response, sending a signed-in user's navigation to every guarded page to the server, and a guarded first page would run its narrowed body with no user.
- When the browser refuses a page, it asks the server for that page rather than redirecting on its own. That keeps one implementation of the redirect and its `returnTo` encoding (the server's), lets the server decide whatever the mirror can't (roles it doesn't list), and costs a round trip only for the person who was going to be sent away anyway. A client-side redirect would have needed the sign-in page's `returnTo` parameter and its default in the client bundle, and a second, separately tested rule for building the URL.

## Open questions

- OAuth providers and passkeys: the provider interfaces and the stdlib's own callback services.
- Whether `RateLimit` should key on the account rather than the session for signed-in users.
- A dedicated guard for token-authenticated endpoints (`requires BearerToken(…)`), instead of checking the token in the handler.
