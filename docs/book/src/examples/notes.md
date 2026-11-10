# Notes: a full-stack app with accounts

`examples/notes` is a notes app with everything a real full-stack app needs: accounts with email and
password or a passkey, notes with tags in SQLite, a list that stays live across browser tabs, editing that
never silently overwrites a change made elsewhere, and a private vault whose text the server can't read.

Where the [weather example](weather.md) talks to someone else's API, this one owns both ends: a
`protocol` between its browser and its server, entities and migrations, sessions and guards.

```bash
k test examples/notes     # integration tests on the real stack, in-process
k dev examples/notes      # http://localhost:8080; the database is .k/notes.db, made at startup
```

## The layout

```text
notes/
├── App.k                   @Layout(AppLayout), @SignInPage(.signIn)
├── migrations/             0001_create_accounts_and_notes.k, 0002_add_note_pinning.k,
│                           0003_add_the_private_vault.k, 0004_add_passkeys.k
├── schema.lock  routes.lock  rpc.lock
└── src/
    ├── accounts/           AccountId, Account, AccountRepository, AuthProtocol, AuthApi,
    │                       AuthService, Credentials, SignUpRequest, SignIn, SignUp,
│                       Passkey, PasskeyRepository, PasskeyService, PasskeySignUp, PasskeySignIn
    ├── notes/              Note, Tag, repositories, NotesProtocol, NotesApi, NoteChanges,
    │                       NotesService, NoteDraft, the Notes, NewNote and EditNote pages
    ├── search/             Searchable, an interface with a default method
    ├── shell/              AppLayout, OfflineBanner and NotesTheme
    ├── vault/              Vault, VaultProtocol, VaultApi, VaultService, the PrivateNote page
    │                       and its PrivateTextEditor
    └── testing/            Samples: AccountId.ada, NoteSummary fixtures
```

Each feature module pairs a server half with a browser half: `NotesApi` implements `NotesProtocol` on
the server, and `NotesService` wraps the protocol in the browser.

```k
// examples/notes/App.k
@Layout(AppLayout)
@SignInPage(.signIn)
public app KNotes {
    metadata {
        manifest(name: "K Notes", shortName: "Notes")
        themeColor(.primary)
        locale(.english)
    }
}
```

## The contract

```k
// examples/notes/src/notes/NotesProtocol.k
public protocol NotesProtocol requires Authenticated {
    // ...
    rpc watch() throws RpcOnlyError -> Stream<NoteChange>
    // ...
    rpc get(id: Uuid) throws LookupError -> NoteSummary
    // ...
    rpc create(draft: NoteDraft) throws RpcOnlyError -> NoteSummary
    // ...
    rpc update(id: Uuid, draft: NoteDraft, revision: Int) throws SaveError -> NoteSummary
    // ...
    rpc setPinned(id: Uuid, isPinned: Bool) throws LookupError -> NoteSummary
    // ...
    rpc delete(id: Uuid) throws RpcOnlyError
}
```

`requires Authenticated` on the protocol guards every rpc, and narrows `session.user` to a non-optional
`SessionUser` in every handler ([Authentication and sessions](../apps/auth.md#narrowing-under-authenticated)).
`watch` is a server-streaming rpc ([Server streaming](../apps/streaming.md)).

## The data

```k
// examples/notes/src/notes/Note.k
public entity Note {
    // ...
    @Id let id: Uuid
    // ...
    @Index let ownerId: AccountId
    // ...
    var title: String
    // ...
    var body: String
    // ...
    var isPinned: Bool = false
    // ...
    @Version var revision: Int = 1
    // ...
    let createdAt: Date
    // ...
    var updatedAt: Date
    // ...
    @Relation var tags: List<Tag> = []
}
```

- `ownerId` is the app's typed user id, `AccountId`, so a note's owner can never be confused with
  another id.
- `@Version` makes `update` a conditional write: it succeeds only when the stored revision still
  equals the one the edit started from ([Persistence](../apps/persistence.md#writes)).
- `isPinned` arrived later, with `migrations/0002_add_note_pinning.k`, which `k migrate new` wrote from
  the difference between the entity and `schema.lock`.

## Validation shared by form and server

`NoteDraft` is both the editor's form value and the rpc's parameter. Its constraints run in the browser
as you type and again on the server before the handler runs:

```k
// examples/notes/src/notes/NoteDraft.k
public struct NoteDraft implements Codable, Equatable, Validatable {
    // ...
    @NotBlank(message: "Give the note a title")
    @Length(max: 120, message: "Keep the title under 120 characters")
    var title: String
    // ...
    func validate(rules: Rules) {
        rules.check(tagLabels().count <= 10, field: .tags, message: "Use at most 10 tags")
    }
    // ...
}
```

See [Forms and validation](../apps/forms.md).

## Edits that never overwrite each other

The server half of `update` relies on `@Version`:

```k
// examples/notes/src/notes/NotesApi.k
let saved = await notes.update(edited)

guard saved else {
    guard let latest = await notes.find(id: id, ownerId: ownerId) else {
        throw SaveError.notFound
    }

    throw SaveError.conflict(latest: summarize(note: latest))
}
```

and the page turns a conflict into a choice instead of an error:

```k
// examples/notes/src/notes/EditNote.k
do {
    latest = try await notes.update(note: current, draft: draft)
    conflict = nil
    isSaved = true
} catch .conflict(let newer) {
    conflict = EditConflict(theirs: newer, mine: draft)
} catch .notFound {
    Router.navigate(to: .notes)
```

## A list that stays live

`NotesApi.watch()` subscribes to `NoteChanges`, a `@Scope(.server)` service holding a
`Broadcast<AccountId, NoteChange>`, before it reads the snapshot, so nothing saved in between is lost.
Every write publishes inside its transaction, and the publish waits for the commit. The page follows
the stream from an effect, and reconnects on demand. [Server streaming](../apps/streaming.md) shows each
piece.

## Offline

`AppLayout` shows an `OfflineBanner` while `Connectivity.isOnline` is `false`: when the browser loses
its network, or a call to the server just failed for want of one. The pages still handle
`rpcFailure(.offline)` on every call, because the banner is a hint and a call can fail anyway. Tests
take the browser offline with `TestConnectivity.goOffline()`, which also makes their rpcs fail; see
[The page and the device](../apps/environment.md).

## Accounts

`AuthApi` hashes passwords with Argon2id, verifies against a hash even when the email is unknown (so
response time reveals nothing), and lets the unique index on the email decide a race between two
sign-ups. [Authentication and sessions](../apps/auth.md) walks through it.

## Passkeys

Next to the password, the sign-up page offers **Sign up with a passkey** and the sign-in page **Sign in
with a passkey** ([Authentication and sessions](../apps/auth.md#passkeys)). A passkey account has no
password: its `passwordHash` is `Password.unusableHash`, which no password matches.

```k
// examples/notes/src/accounts/AuthApi.k
    rpc signUpWithPasskey(email: EmailAddress, registration: PasskeyRegistration) async throws SignUpError {
        // Every check (the session's challenge, the origin, the user
        // verification, the key) passes, or nothing is stored.
        let credential = try await verified(registration)
```

- **The challenge comes first.** `PasskeySignUp` and `PasskeySignIn` fetch their options from an
  `@Effect` as the page opens, because the device's sheet may only open from the click; after any
  attempt, `PasskeyService` fetches the next challenge, since each one is good once.
- **The account and its `Passkey` row are inserted in one transaction,** and both the email and the
  credential id are `@Unique`, so a race creates neither twice (`migrations/0004_add_passkeys.k`).
- **Every refusal reads the same to the browser:** `SignUpError.passkeyNotAccepted` and
  `SignInError.passkeyNotAccepted` say "not accepted" whichever check failed, and the server keeps the
  reason.
- `PasskeyService.test.k` signs up, out and in with `TestAuthenticator`, and checks that another site's
  passkey, a dismissed sheet and a cloned passkey's counter are refused.

## The private vault

A note can have a **private text**, sealed in the browser. The server stores and syncs it but can't read
it, and neither can a database dump or an operator. The title and tags stay plain, so the list and search
keep working; the private text is the only private part ([Bytes and cryptography](../apps/crypto.md)).

```k
// examples/notes/src/vault/VaultService.k
func create(passphrase: String) async throws VaultError {
    let salt = PassphraseSalt.generate()
    let wrapping = await AesGcmKey.derive(fromPassphrase: passphrase, salt: salt)
    let vault = await AesGcmKey.generateExportable()

    try await api.create(vault: StoredVault(salt: salt, wrappedKey: await wrapping.wrap(vault)))
    await remember(vault.key)
}
// ...
func open(_ sealed: Sealed<PrivateText>, note id: Uuid) async throws VaultError -> PrivateText {
    let key = try unlockedKey()
    do {
        return try await key.open(sealed, context: "note:{id}".utf8)
    } catch .authenticationFailed, .undecodable {
        throw VaultError.tampered
    }
}
```

- **The vault key never leaves the browser in the clear.** It is generated there, sent to the server only
  wrapped with a key derived from the vault passphrase (`StoredVault`), and kept in this browser's
  `KeyStore` as a non-extractable key. `AesGcmKey` isn't `Codable`, so no rpc, `@Load` field or log can
  carry it. A wrong passphrase is the `.authenticationFailed` of `unwrap`, so the server keeps no
  password verifier.
- **The vault has its own passphrase, never the sign-in password,** which the server receives at sign-in.
- **Each ciphertext is bound to its note:** `"note:{id}"` is authenticated, so a server that moves a
  ciphertext to another note produces `VaultError.tampered`, not the wrong note.
- **The server's side is ordinary data.** `Vault` stores the salt and the wrapped key, and `Note` the
  sealed text, as text columns (`migrations/0003_add_the_private_vault.k`); `VaultApi` checks only that
  the note is the caller's.
- `PrivateTextEditor` is `@Rendering(.client)`, because the key exists only in the browser: the server
  renders "Opening the vault…" and the browser restores the kept key after hydration.
- **What it doesn't protect against:** the server serves the JavaScript, so a malicious server could ship
  code that reads the passphrase. That is true of every web end-to-end design.

## The tests

Every test runs the browser and the server in one process, against a fresh in-memory SQLite database
with every migration applied. A test signs in without creating an account:

```k
// examples/notes/src/notes/Notes.test.k
test "the list shows the account's notes and filters as you type" {
    await sessions.signIn(userId: AccountId.ada)
    try await notes.create(draft: draft(title: "Groceries", body: "Milk and eggs", tags: "home"))
    try await notes.create(draft: draft(title: "Ideas", tags: "work"))
    let screen = render(Notes())
    await screen.settle()

    expect screen.link(named: "Groceries").isVisible
    expect screen.link(named: "Ideas").isVisible
    // ...
}
```

`AccountId.ada` comes from `src/testing/Samples.k`, a test-only module that no production code can
import. The vault's tests run on `MemoryKeyStore`, the test default for `KeyStore`, and the real
cryptography with fast test salts:

```k
// examples/notes/src/vault/VaultService.test.k
test "the server can't move a ciphertext to another note" {
    await sessions.signIn(userId: AccountId.ada)
    try await vault.create(passphrase: "correct horse")
    let sealed = try await vault.seal(PrivateText(body: "hi"), note: Uuid("00000000-0000-4000-8000-000000000001"))

    expect throws VaultError.tampered {
        try await vault.open(sealed, note: Uuid("00000000-0000-4000-8000-000000000002"))
    }
}
```
