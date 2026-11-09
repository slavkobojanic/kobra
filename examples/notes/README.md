# K Notes

A small full-stack notes app written entirely in K: accounts with email and password, notes with tags, a list that stays live across browser tabs, editing that never silently overwrites a change made elsewhere, and a private vault whose text the server can't read. Data lives in SQLite.

It is a teaching example. Every file is meant to show one part of the language the way the design documents in [`docs/design`](../../docs/design/README.md) intend it to be written, with the syntax of the compiler's [grammar](../../docs/compiler/grammar.md). [Design documents used](#design-documents-used) lists the documents each part relies on.

## What it demonstrates

| Feature | Where |
|---|---|
| A `protocol` with unary `rpc`s and a **server-streaming** `rpc` (`Stream<T>`) | `src/notes/NotesProtocol.k` |
| A server `service … implements Protocol`, with transactions whose announcements wait for the commit | `src/notes/NotesApi.k`, `src/accounts/AuthApi.k` |
| A client `service` wrapping a protocol: state, optimistic updates, merging a live stream | `src/notes/NotesService.k`, `src/accounts/AuthService.k` |
| Fan-out between requests with a process-wide service (`@Scope(.server)`, a keyed `Broadcast<Key, T>`) | `src/notes/NoteChanges.k` |
| `entity` and `repository` with LINQ-style queries | `src/notes/Note.k`, `src/notes/NoteRepository.k`, `src/accounts/AccountRepository.k` |
| Relations loaded with `.including`, a bulk `.update { … }` and a bulk `.delete()` | `src/notes/Tag.k`, `src/notes/NoteRepository.k`, `src/notes/TagRepository.k` |
| Optimistic locking with `@Version`, and a typed unique violation instead of a racy pre-check | `src/notes/Note.k`, `src/notes/NotesApi.k`, `src/accounts/AuthApi.k` |
| A schema migration recorded in `schema.lock` | `migrations/0002_add_note_pinning.k`, `schema.lock` |
| Bean-Validation-style constraints, a cross-field rule, an async `@Check`, a typed `EmailAddress` field with `@Format`, and `@Form` starting from `.empty` or `.empty(…)`, with server-side validation failures shown under their fields | `src/accounts/Credentials.k`, `src/accounts/SignUpRequest.k`, `src/accounts/SignUp.k`, `src/notes/NoteDraft.k`, `src/notes/NoteEditor.k` |
| `page`, `layout`, typed navigation with `link(to:)`, `@Param`, a named URL | `src/notes/Notes.k`, `src/notes/EditNote.k`, `src/shell/AppLayout.k` |
| `requires Authenticated` on pages and on a whole protocol, `RateLimit`, a typed `AuthUserId`, `Session` and `SessionManager` | `src/notes/NotesProtocol.k`, `src/accounts/AuthProtocol.k`, `src/accounts/AccountId.k`, `src/accounts/AuthApi.k`, `App.k` |
| `@Load` with `loading` and an exhaustive `failed` block that answers `404`, sets its own title and retries with `reload()` | `src/notes/EditNote.k` |
| Structs (wire types, form values, a typed id), enums with associated data, an `interface` with a default method | `src/notes/NoteSummary.k`, `src/notes/NoteChange.k`, `src/search/Searchable.k` |
| Typed, checked errors and exhaustive `catch`, including the `rpcFailure(RpcFailure)` case every rpc has, for refused, invalid and offline calls, and `RpcOnlyError` for rpcs with no errors of their own | `src/notes/SaveError.k`, `src/notes/NotesProtocol.k`, `src/notes/EditNote.k`, `src/notes/Notes.k`, `src/notes/NotesService.k` |
| An offline banner from the reactive `Connectivity` service, tested with `TestConnectivity.goOffline()`, which also fails the test's rpcs | `src/shell/OfflineBanner.k`, `src/shell/OfflineBanner.test.k`, `src/notes/NewNote.test.k` |
| The injected `Clock` for timestamps and relative times, and `TestClock` in tests | `src/notes/NotesApi.k`, `src/accounts/AuthApi.k`, `src/notes/NoteCard.k`, `src/notes/NoteCard.test.k` |
| `@Emits` events, function-typed props, typed scoped styles, theme tokens, ARIA attributes, `VisuallyHidden` | `src/notes/NoteCard.k`, `src/notes/NoteEditor.k`, `src/shell/NotesTheme.k` |
| An end-to-end encrypted **private vault**: a note's text sealed in the browser with `AesGcmKey`, the vault key wrapped with a passphrase-derived key on the server and kept in the browser's `KeyStore`, ciphertexts bound to their note | `src/vault/VaultService.k`, `src/vault/VaultApi.k`, `src/vault/PrivateTextEditor.k`, `src/vault/VaultService.test.k`, `migrations/0003_add_the_private_vault.k` |
| Integration tests on the real stack in-process, component and page tests, and fixtures in a test-only module | every `*.test.k`, `src/testing/Samples.k` |

## How it works

### Live updates across tabs

`NotesProtocol.watch()` returns a `Stream<NoteChange>`. Its first message is a `snapshot` of every note; after that it sends `saved` and `deleted` changes.

1. The `Notes` page starts `NotesService.watch()` from an `@Effect(on: [connectAttempt])`. It runs once the page mounts, and again when the person presses **Reconnect** after a network failure. It runs in the browser only, and leaving the page cancels it, which closes the stream.
2. On the server, `NotesApi.watch()` subscribes to `NoteChanges` **before** reading the snapshot, so a save that lands in between is delivered after the snapshot instead of being lost.
3. Every write in `NotesApi` publishes on `NoteChanges`, a `@Scope(.server)` service with one instance per server process. Its `Broadcast<AccountId, NoteChange>` is keyed by owner, so a browser only ever receives its own account's changes. A publish inside `database.transaction` is held until the commit and dropped on rollback, so other tabs never see a change that didn't happen.
4. `NotesService.apply(change:)` is idempotent. A tab applies its own writes immediately and again when the stream echoes them back, with the same result.
5. Signing out ends the session, and the runtime closes every stream opened under it. A stream that ends isn't an error, and a dropped connection (or a server restart, which ends open streams at once) ends it the same way, so `NotesService.watch()` opens a new one. After a sign-out in another tab, the first response of that call tells the browser the session is gone, and the router leaves the page at once, because `Notes` requires a signed-in user: it reloads the page, and the server sends the tab to the sign-in page. The page's own `catch .rpcFailure(.rejected)` is there for the case where the browser's session copy is still current, but it doesn't run here. After a dropped connection, the new stream's snapshot brings the list up to date, or the call fails with `.offline` and the page offers to reconnect.

### Conflict-safe editing

Every note has a `@Version var revision`. `EditNote` sends the revision it started from with each save, and `NotesApi.update` writes with it: the inherited `update(note)` only changes the row when the stored revision still matches, and then increments it. If another tab saved in the meantime, the update returns `false`, nothing is overwritten, and the rpc throws `SaveError.conflict(latest:)`. The page then shows a choice: **Keep my version** saves again on top of the newer revision, and **Discard my changes** returns to the list.

Pinning isn't an edit. `NoteRepository.setPinned` is a bulk update that writes only `isPinned`, so it never bumps the revision, never causes a conflict, and can't overwrite an edit saved at the same moment.

### Accounts

Emails are `EmailAddress` values. `EmailAddress.parse` trims what was typed and lower-cases the domain, and `@Format` sets the message for text that isn't an address. `AuthApi` hashes passwords with `k.crypto.Password` (Argon2id), stores and compares the address lower-cased in full, so `Ada@example.com` and `ada@example.com` are one account, and verifies against an unusable hash when the email is unknown, so response times don't reveal which emails are registered. The email column is `@Unique`, so a second sign-up with the same address makes `insert` throw `Account.UniqueViolation.email`, which becomes `SignUpError.emailTaken`. There is no check-then-insert race.

Signing in calls the server-only `SessionManager.signIn(userId:)` with the account's `AccountId`, the app's one `AuthUserId`, so `SessionUser.id` and every note's `ownerId` are typed ids. The browser's `Session` mirrors the result, so `AuthService.user` and the layout update without another request.

`NotesProtocol` declares `requires Authenticated` once, for every rpc. Inside `NotesApi`, `session.user` is therefore never `nil`, and every query is scoped to that user, so another account's note behaves exactly like one that doesn't exist. A refusal reaches the browser as the typed `rpcFailure(.rejected(…))` case of each rpc's error enum, and pages answer it by navigating to `.signIn.returnTo(…)`. A draft or form that fails validation on the server arrives as `rpcFailure(.invalid(…))`, which the pages pass back to their `@Form` so it appears under the fields. When the browser can't reach the server, the same case carries `.offline` or `.timeout`, and the page shows an alert while the form keeps what was typed. Rpcs with no errors of their own, such as `watch`, `create` and `delete`, throw `RpcOnlyError`, the stdlib enum with only that case.

### The migration

The first version of the app had no pinning. Adding `isPinned` to `Note` and running `k migrate new "add note pinning"` generated `migrations/0002_add_note_pinning.k` from the difference between the entities and `schema.lock`, and updated the lock. `k check` fails if the entities, the migrations and the lock ever disagree.

## Structure

Each directory under `src/` is a module. Each file holds one public declaration named after the file, whose members are public unless marked `private`; helpers in the same file are visible only within the module.

```
App.k                    the app: default layout, sign-in page, app-wide metadata
k.toml                   project manifest
.env.example             optional overrides for `k dev` (another DATABASE_URL); copy to the git-ignored .env
schema.lock              database schema, generated by `k migrate`
migrations/
  0001_create_accounts_and_notes.k
  0002_add_note_pinning.k
src/
  accounts/              module accounts: signing up, in and out
    Account.k              entity (server-only)
    AccountId.k            struct: the typed account and session user id
    AccountRepository.k    repository
    AuthProtocol.k         protocol: isEmailAvailable, signUp, signIn, signOut
    AuthApi.k              server implementation, using SessionManager
    AuthService.k          client service; exposes the session user
    Credentials.k          struct: the sign-in form
    SignUpRequest.k        struct: the sign-up form, with an async @Check
    SignInError.k          enum
    SignUpError.k          enum
    SignIn.k               page
    SignUp.k               page
  notes/                 module notes: everything about notes
    Note.k, Tag.k          entities (server-only); Note.tags is a relation
    NoteRepository.k       repository with LINQ-style queries, .including and a bulk update
    TagRepository.k        repository with a bulk delete
    NotesProtocol.k        protocol, including the watch() stream
    NotesApi.k             server implementation
    NoteChanges.k          @Scope(.server) fan-out of changes to open streams
    NotesService.k         client service: live list that reopens ended streams, optimistic pinning
    NoteSummary.k          struct: the wire type for a note (Searchable)
    NoteDraft.k            struct: the editor's form value, with validation
    NoteChange.k           enum: messages on the stream
    LookupError.k, SaveError.k
                           enums: typed errors, each with an rpcFailure(RpcFailure) case
    NoteCard.k             component: one note in the list, with @Emits
    NoteEditor.k           component: the @Form shared by NewNote and EditNote
    Notes.k                page at "/": the live, filterable list
    NewNote.k              page
    EditNote.k             page: @Param, @Load, loading and failed, conflicts
  search/                module search
    Searchable.k           interface with a default method
  shell/                 module shell: what surrounds every page
    AppLayout.k            layout
    NotesTheme.k           theme
  testing/               test-only module: only *.test.k files can see it
    Samples.k              fixtures: AccountId.ada and AccountId.ben
```

Server-only declarations (entities, repositories, `AuthApi`, `NotesApi`, `NoteChanges`) sit next to the client code that uses their results. The compiler splits the bundles and rejects any client import of server-only code.

## Running it

From this directory:

```bash
k dev
```

Open http://localhost:8080 and sign up. That is all the setup there is: `k dev` builds the app, creates a SQLite database at `.k/notes.db` (git-ignored), applies the migrations, serves on port 8080 (`--port` to change) and rebuilds and restarts on every change. There is no hot module replacement yet, so refresh the page after a change; accounts, sessions and notes are in the database and survive the restart.

The other commands ([13-tooling.md](../../docs/design/13-tooling.md)):

```bash
k test                 # every *.test.k file; add --watch to re-run on save
k check                # types, tests, and schema/protocol/route lockfile checks
k build                # dist/app: one self-contained server executable plus hashed client assets
k run                  # like k dev, without rebuilding on change
```

- `k dev` reads `DATABASE_URL` from the environment or a `.env` file when one is set (`.env.example` shows a Postgres one), and defaults to `sqlite:.k/notes.db`. It applies pending migrations at startup.
- `k test` never reads the environment. Every test gets a fresh in-memory SQLite database with the migrations applied, so a test sees only what it wrote.
- Sessions are stored server-side by `k.auth`, so there is no session secret to set.
- In production, `dist/app` reads `DATABASE_URL` (Postgres, as [19-persistence.md](../../docs/design/19-persistence.md) intends) and, optionally, `DATABASE_PASSWORD` or `DATABASE_PASSWORD_FILE` from the real environment, and refuses to start without `DATABASE_URL`: the development default never applies there. `dist/app config` lists every variable it reads.
- Several K apps can run on `localhost` at once: on an `http` origin with a port the session cookie is named after the port (`k_session_8080`), so the apps don't sign each other out.

Try it with two tabs: sign up, open the list in both, and create, pin or delete a note in one. Then open the same note's editor in both tabs and save in each.

## Tests

| File | Kind | Covers |
|---|---|---|
| `accounts/AuthService.test.k` | integration | sign-up signs in, emails in any letter case, wrong password and unknown email refused alike, availability, sign-out |
| `accounts/AuthApi.test.k` | handler | a duplicate email is refused by the unique index even when validation is bypassed (a sign-up race) |
| `accounts/SignIn.test.k`, `accounts/SignUp.test.k` | page | field errors, an email typed with spaces and capitals, cross-field rule, async `@Check`, alerts, navigation to `returnTo` |
| `notes/NotesService.test.k` | integration | trimming and tag parsing, revisions, conflicts, pinning isn't an edit, idempotent delete, ordering, isolation between accounts, guard rejection, applying stream changes |
| `notes/NotesApi.test.k` | integration | the stream's snapshot, ordering of later changes, no cross-account notes or events, the stream ending on sign-out, nothing announced for refused writes, every rpc guarded |
| `notes/Notes.test.k`, `notes/EditNote.test.k` | page | live update from "another tab", filtering, pinning, redirect when signed out, redirect after signing out in another tab, saving, conflict resolution, missing notes, deleting |
| `notes/NoteCard.test.k`, `notes/NoteEditor.test.k` | component | rendering, the relative edit time, `aria-pressed` and each pin toggle's accessible name, emitted events, validation messages, the ten-tag limit with duplicates |
| `notes/NoteDraft.test.k`, `search/Searchable.test.k` | unit | tag parsing, draft round-trip, search matching |

Integration tests use no fakes. They inject the client services and run through the protocols in-process, through the real wire format, to the real server implementations and SQLite. To act as a user without creating an account, they inject `SessionManager` and call `sessions.signIn(userId: AccountId.ada)`, with the fixture ids from `src/testing/Samples.k`. Time comes from `TestClock`, the default for `Clock` in every `k test`, so timestamps and orderings are exact; tests reach it with `@Inject let clock: TestClock` and need no `use` line. The server's `clock.now` and the test's share one virtual time, and test helpers that build notes take `updatedAt: clock.now`, because a plain function can't read the clock.

Calls in tests run in-process, so `.offline` and `.timeout` never happen there, and the pages' handling of them isn't covered by a test.

## Design documents used

| Document | Used for |
|---|---|
| [26-auth-and-sessions.md](../../docs/design/26-auth-and-sessions.md) | `Session`, `SessionManager`, `AuthUserId`, narrowing under `Authenticated`, `RateLimit`, `Password`, `@SignInPage` |
| [28-rpc-wire-format.md](../../docs/design/28-rpc-wire-format.md) | `rpcFailure(RpcFailure)` in every rpc's error enum, `RpcOnlyError`, `.offline` and `.timeout` |
| [29-server-streaming.md](../../docs/design/29-server-streaming.md) | implementing `watch()`, keyed `Broadcast`, `@Scope(.server)`, `for await`, following a stream from an effect, re-subscribing after a stream ends |
| [19-persistence.md](../../docs/design/19-persistence.md) | `insert`/`update`/`delete`, `@Version`, unique violations, bulk `.update` and `.delete()`, `single`, `thenByDescending`, `Database.transaction`, migration files, table naming, `@Index`, the `schema.lock` format |
| [15-forms-and-validation.md](../../docs/design/15-forms-and-validation.md) | constraints, `@Check`, `@Format`, `@Form` with `.empty` and `.empty(…)`, `form.value` inside `handle`, passing `rpcFailure(.invalid(…))` back to the form |
| [31-email.md](../../docs/design/31-email.md) | `EmailAddress` and how `parse` normalizes it |
| [08-types.md](../../docs/design/08-types.md) | a lambda's last expression as its value |
| [23-app-structure.md](../../docs/design/23-app-structure.md) | `App.k` |
| [25-config-and-secrets.md](../../docs/design/25-config-and-secrets.md) | `DATABASE_URL` and `.env.example` |
| [24-projects.md](../../docs/design/24-projects.md) | `k.toml` and the project layout |
| [16-modules.md](../../docs/design/16-modules.md) | member visibility, the test-only `testing` module |
| [39-doc-comments.md](../../docs/design/39-doc-comments.md) | `///` documentation comments |
| [21-testing.md](../../docs/design/21-testing.md) | screen queries, `settle()`, `currentPage`, hermetic defaults, test-file helpers, fixtures |
| [45-web-platform.md](../../docs/design/45-web-platform.md) | `Connectivity` and the offline banner, `TestConnectivity` |
| [35-time.md](../../docs/design/35-time.md) | the injected `Clock`, `formatted(as: .relative, from:)`, `TestClock` |
| [05-views.md](../../docs/design/05-views.md) | `.pressed`, `.role`, `.label`, `VisuallyHidden` |
| [14-async-data.md](../../docs/design/14-async-data.md), [22-metadata.md](../../docs/design/22-metadata.md) | `@Load`, `failed`, `reload()`, `status(.notFound)`, metadata when a load fails |
