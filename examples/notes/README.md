# K Notes

A small full-stack notes app written entirely in K: accounts with email and password, notes with tags, a list that stays live across browser tabs, and editing that never silently overwrites a change made elsewhere. Data lives in SQLite.

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
| Bean-Validation-style constraints, a cross-field rule, an async `@Check`, and `@Form`, with server-side validation failures shown under their fields | `src/accounts/SignUpRequest.k`, `src/accounts/SignUp.k`, `src/notes/NoteDraft.k`, `src/notes/NoteEditor.k` |
| `page`, `layout`, typed navigation with `link(to:)`, `@Param`, a named URL | `src/notes/Notes.k`, `src/notes/EditNote.k`, `src/shell/AppLayout.k` |
| `requires Authenticated` on pages and on a whole protocol, `RateLimit`, a typed `AuthUserId`, `Session` and `SessionManager` | `src/notes/NotesProtocol.k`, `src/accounts/AuthProtocol.k`, `src/accounts/AccountId.k`, `src/accounts/AuthApi.k`, `App.k` |
| `@Load` with `loading` and an exhaustive `failed` block that answers `404` | `src/notes/EditNote.k` |
| Structs (wire types, form values, a typed id), enums with associated data, an `interface` with a default method | `src/notes/NoteSummary.k`, `src/notes/NoteChange.k`, `src/search/Searchable.k` |
| Typed, checked errors and exhaustive `catch`, including the `rpcFailure(RpcFailure)` case for refused and invalid calls | `src/notes/SaveError.k`, `src/notes/EditNote.k`, `src/notes/NotesService.k` |
| `@Emits` events, function-typed props, typed scoped styles, theme tokens, ARIA attributes, `VisuallyHidden` | `src/notes/NoteCard.k`, `src/notes/NoteEditor.k`, `src/shell/NotesTheme.k` |
| Integration tests on the real stack in-process, component and page tests, and fixtures in a test-only module | every `*.test.k`, `src/testing/Samples.k` |

## How it works

### Live updates across tabs

`NotesProtocol.watch()` returns a `Stream<NoteChange>`. Its first message is a `snapshot` of every note; after that it sends `saved` and `deleted` changes.

1. The `Notes` page starts `NotesService.watch()` from an `@Effect(on: [])`. It runs once per visit, in the browser only, and leaving the page cancels it, which closes the stream.
2. On the server, `NotesApi.watch()` subscribes to `NoteChanges` **before** reading the snapshot, so a save that lands in between is delivered after the snapshot instead of being lost.
3. Every write in `NotesApi` publishes on `NoteChanges`, a `@Scope(.server)` service with one instance per server process. Its `Broadcast<AccountId, NoteChange>` is keyed by owner, so a browser only ever receives its own account's changes. A publish inside `database.transaction` is held until the commit and dropped on rollback, so other tabs never see a change that didn't happen.
4. `NotesService.apply(change:)` is idempotent. A tab applies its own writes immediately and again when the stream echoes them back, with the same result.
5. Signing out ends the session, and the runtime closes every stream opened under it.

### Conflict-safe editing

Every note has a `@Version var revision`. `EditNote` sends the revision it started from with each save, and `NotesApi.update` writes with it: the inherited `update(note)` only changes the row when the stored revision still matches, and then increments it. If another tab saved in the meantime, the update returns `false`, nothing is overwritten, and the rpc throws `SaveError.conflict(latest:)`. The page then shows a choice: **Keep my version** saves again on top of the newer revision, and **Discard my changes** returns to the list.

Pinning isn't an edit. `NoteRepository.setPinned` is a bulk update that writes only `isPinned`, so it never bumps the revision, never causes a conflict, and can't overwrite an edit saved at the same moment.

### Accounts

`AuthApi` hashes passwords with `k.crypto.Password` (Argon2id), compares emails trimmed and lower-cased, and verifies against an unusable hash when the email is unknown, so response times don't reveal which emails are registered. The email column is `@Unique`, so a second sign-up with the same address makes `insert` throw `Account.UniqueViolation.email`, which becomes `SignUpError.emailTaken`. There is no check-then-insert race.

Signing in calls the server-only `SessionManager.signIn(userId:)` with the account's `AccountId`, the app's one `AuthUserId`, so `SessionUser.id` and every note's `ownerId` are typed ids. The browser's `Session` mirrors the result, so `AuthService.user` and the layout update without another request.

`NotesProtocol` declares `requires Authenticated` once, for every rpc. Inside `NotesApi`, `session.user` is therefore never `nil`, and every query is scoped to that user, so another account's note behaves exactly like one that doesn't exist. A refusal reaches the browser as the typed `rpcFailure(.rejected(…))` case of each rpc's error enum, and pages answer it by navigating to `.signIn.returnTo(…)`. A draft or form that fails validation on the server arrives as `rpcFailure(.invalid(…))`, which the pages pass back to their `@Form` so it appears under the fields.

### The migration

The first version of the app had no pinning. Adding `isPinned` to `Note` and running `k migrate new "add note pinning"` generated `migrations/0002_add_note_pinning.k` from the difference between the entities and `schema.lock`, and updated the lock. `k check` fails if the entities, the migrations and the lock ever disagree.

## Structure

Each directory under `src/` is a module. Each file holds one public declaration named after the file, whose members are public unless marked `private`; helpers in the same file are visible only within the module.

```
App.k                    the app: default layout, sign-in page, app-wide metadata
k.toml                   project manifest
.env.example             DATABASE_URL for `k dev`; copy to the git-ignored .env
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
    NotesService.k         client service: live list, optimistic pinning
    NoteSummary.k          struct: the wire type for a note (Searchable)
    NoteDraft.k            struct: the editor's form value, with validation
    NoteChange.k           enum: messages on the stream
    AccessError.k, LookupError.k, SaveError.k
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

These commands are part of the K toolchain design ([13-tooling.md](../../docs/design/13-tooling.md)); the compiler is still being built.

```bash
cp .env.example .env   # once: points DATABASE_URL at a SQLite file under .k/
k dev                  # dev server with state-preserving hot reload
k test                 # every *.test.k file; add --watch to re-run on save
k check                # types, formatting, tests, and schema/protocol/route lockfile checks
k build                # dist/app: one self-contained server executable plus hashed client assets
```

- `k dev` reads `DATABASE_URL` from `.env` and applies pending migrations at startup.
- `k test` never reads the environment. It applies the migrations to a fresh in-memory SQLite database and runs each test in a transaction that is rolled back afterwards.
- Sessions are stored server-side by `k.auth`, so there is no session secret to set.
- In production, `dist/app` reads `DATABASE_URL` (Postgres, as [19-persistence.md](../../docs/design/19-persistence.md) intends) and `DATABASE_PASSWORD_FILE` from the real environment, and refuses to start if they are missing. `dist/app config` lists every variable it reads.

Try it with two tabs: sign up, open the list in both, and create, pin or delete a note in one. Then open the same note's editor in both tabs and save in each.

## Tests

| File | Kind | Covers |
|---|---|---|
| `accounts/AuthService.test.k` | integration | sign-up signs in, case-insensitive emails, wrong password and unknown email refused alike, availability, sign-out |
| `accounts/AuthApi.test.k` | handler | a duplicate email is refused by the unique index even when validation is bypassed (a sign-up race) |
| `accounts/SignIn.test.k`, `accounts/SignUp.test.k` | page | field errors, cross-field rule, async `@Check`, alerts, navigation to `returnTo` |
| `notes/NotesService.test.k` | integration | trimming and tag parsing, revisions, conflicts, pinning isn't an edit, idempotent delete, ordering, isolation between accounts, guard rejection, applying stream changes |
| `notes/NotesApi.test.k` | integration | the stream's snapshot, ordering of later changes, no cross-account notes or events, the stream ending on sign-out, nothing announced for refused writes, every rpc guarded |
| `notes/Notes.test.k`, `notes/EditNote.test.k` | page | live update from "another tab", filtering, pinning, redirect when signed out, saving, conflict resolution, missing notes, deleting |
| `notes/NoteCard.test.k`, `notes/NoteEditor.test.k` | component | rendering, `aria-pressed` and each pin toggle's accessible name, emitted events, validation messages, the ten-tag limit with duplicates |
| `notes/NoteDraft.test.k`, `search/Searchable.test.k` | unit | tag parsing, draft round-trip, search matching |

Integration tests use no fakes. They inject the client services and run through the protocols in-process, through the real wire format, to the real server implementations and SQLite. To act as a user without creating an account, they inject `SessionManager` and call `sessions.signIn(userId: AccountId.ada)`, with the fixture ids from `src/testing/Samples.k`. Time comes from `TestClock`, the hermetic default in every `k test`, so timestamps and orderings are exact; tests reach it with `@Inject let clock: TestClock` and need no `use` line.

## Design documents used

| Document | Used for |
|---|---|
| [26-auth-and-sessions.md](../../docs/design/26-auth-and-sessions.md) | `Session`, `SessionManager`, `AuthUserId`, narrowing under `Authenticated`, `RateLimit`, `Password`, `@SignInPage` |
| [28-rpc-wire-format.md](../../docs/design/28-rpc-wire-format.md) | `rpcFailure(RpcFailure)` in every guarded or validated rpc's error enum |
| [29-server-streaming.md](../../docs/design/29-server-streaming.md) | implementing `watch()`, keyed `Broadcast`, `@Scope(.server)`, `for await`, `@Effect(on: [])` |
| [19-persistence.md](../../docs/design/19-persistence.md) | `insert`/`update`/`delete`, `@Version`, unique violations, bulk `.update` and `.delete()`, `single`, `thenByDescending`, `Database.transaction`, migration files, table naming, `@Index`, the `schema.lock` format |
| [15-forms-and-validation.md](../../docs/design/15-forms-and-validation.md) | constraints, `@Check`, `@Form`, passing `rpcFailure(.invalid(…))` back to the form |
| [23-app-structure.md](../../docs/design/23-app-structure.md) | `App.k` |
| [25-config-and-secrets.md](../../docs/design/25-config-and-secrets.md) | `DATABASE_URL` and `.env.example` |
| [24-projects.md](../../docs/design/24-projects.md) | `k.toml` and the project layout |
| [16-modules.md](../../docs/design/16-modules.md) | member visibility, the test-only `testing` module |
| [39-doc-comments.md](../../docs/design/39-doc-comments.md) | `///` documentation comments |
| [21-testing.md](../../docs/design/21-testing.md) | screen queries, `settle()`, `currentPage`, hermetic defaults, test-file helpers, fixtures |
| [35-time.md](../../docs/design/35-time.md) | `TestClock` |
| [05-views.md](../../docs/design/05-views.md) | `.pressed`, `.role`, `.label`, `VisuallyHidden` |
| [14-async-data.md](../../docs/design/14-async-data.md), [22-metadata.md](../../docs/design/22-metadata.md) | `@Load`, `failed`, `status(.notFound)` |
