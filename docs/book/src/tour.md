# A tour of Kobra

This chapter builds a small full-stack feature, a guestbook, from the database table to the button.
It moves quickly and skips details: the point is to show how the pieces of a Kobra app fit together. Every
topic it touches has its own chapter later.

Here is the plan, from the bottom up:

| Layer | Kind | Runs on |
|---|---|---|
| A table of entries | `entity` and `repository` | server |
| A contract between browser and server | `protocol` | both |
| The server implementation of the contract | `service … implements` | server |
| A client-side service that wraps the contract | `service` | both |
| The page people see | `page`, `component` | browser (and server rendering) |

Everything lives in one module, `src/guestbook/`.

## Data that crosses the wire

Start with plain data. A `struct` has value semantics, and `implements Codable` gives it a wire
encoding:

```k
// src/guestbook/GuestbookEntry.k
~import k.crypto.Uuid

/// One guestbook entry, as the browser sees it.
public struct GuestbookEntry implements Codable {
    let id: Uuid
    let name: String
    let message: String
}
```

The request to sign the guestbook is a struct too. Its fields carry **validation constraints**, and the
same struct backs the form in the browser and the request the server receives:

```k
// src/guestbook/SignRequest.k
~import k.validation.Length
~import k.validation.NotBlank
~import k.validation.Validatable

/// What a visitor fills in to sign the guestbook.
public struct SignRequest implements Codable, Validatable {
    @NotBlank @Length(max: 40) var name: String
    @NotBlank @Length(max: 500) var message: String
}
```

## The database

An `entity` describes a table, and a `repository` holds its queries. Queries are ordinary collection
code, and the compiler translates the lambdas into SQL at compile time:

```k
// src/guestbook/Entry.k
~import k.db.Id
~import k.crypto.Uuid
entity Entry {
    @Id let id: Uuid
    var name: String
    var message: String
    var createdAt: Date
}
```

```k
// src/guestbook/EntryRepository.k
repository EntryRepository for Entry {
    func latest() async -> List<Entry> {
        return await entries
            .sortedByDescending { entry => entry.createdAt }
            .limit(100)
    }
}
```

Inside the repository, `entries` is the table. A query is a description until it is awaited, and every
query is awaited explicitly, even one that is returned. A query that can't be translated to SQL is a
compile error, never a silent in-memory scan. `entity` and `repository` are **server-only**: the compiler keeps
them, and anything that touches them, out of the browser bundle.

Generate the migration that creates the table:

```bash
k migrate new "create entries"
```

The toolchain compares your entities with `schema.lock` and writes
`migrations/0001_create_entries.k` for you to review and commit. The table is named after the entity,
`entry`, and the columns after the fields, `created_at` for `createdAt`. Those names are recorded in
`schema.lock`, so renaming the entity or a field later never moves data.

## The contract

A `protocol` declares what the browser can ask the server. It compiles to Protobuf and runs over the
Connect protocol, so you never write a URL, a fetch call or a JSON parser:

```k
// src/guestbook/GuestbookProtocol.k
~import k.rpc.RpcOnlyError

/// What the browser can ask the guestbook server.
public protocol GuestbookProtocol {
    rpc entries() throws RpcOnlyError -> List<GuestbookEntry>

    rpc sign(request: SignRequest) throws RpcOnlyError -> GuestbookEntry
}
```

Every rpc can fail outside its handler. The browser may be offline or the call may time out, and
`sign` takes a `Validatable` request, so the runtime refuses an invalid one before any code of yours
runs. Every failure outside the handler arrives as one typed case, `rpcFailure`, so every rpc declares a
`throws` clause. An rpc with no errors of its own declares `throws RpcOnlyError`, the standard error enum
that has only that case.

## The server

A service that `implements` a protocol is its server implementation. It may inject repositories,
because it is server-only:

```k
// src/guestbook/GuestbookApi.k
~import k.time.Clock
~import k.rpc.RpcOnlyError
~import k.crypto.Uuid
service GuestbookApi implements GuestbookProtocol {
    @Inject let repository: EntryRepository
    @Inject let clock: Clock

    rpc entries() async throws RpcOnlyError -> List<GuestbookEntry> {
        let latest = await repository.latest()

        return latest.map(summarize)
    }

    rpc sign(request: SignRequest) async throws RpcOnlyError -> GuestbookEntry {
        let entry = Entry(id: Uuid.random(), name: request.name, message: request.message, createdAt: clock.now)

        await repository.insert(entry)

        return summarize(entry: entry)
    }

    private func summarize(entry: Entry) -> GuestbookEntry {
        return GuestbookEntry(id: entry.id, name: entry.name, message: entry.message)
    }
}
```

`SignRequest` is `Validatable`, so the runtime validates every request **before** `sign` runs. A
client that skips the form can't skip the rules. The handler itself never throws `rpcFailure`; only the
runtime produces it. The current time comes from the injected `Clock` service: Kobra has no ambient way to
read it, so tests control it.

## The client service

Components never call protocols directly. Remote calls go through a service, which is where caching,
retries and shared state belong. This one is thin for now:

```k
// src/guestbook/GuestbookService.k
~import k.rpc.RpcOnlyError
public service GuestbookService {
    @Inject let api: GuestbookProtocol

    func entries() async throws RpcOnlyError -> List<GuestbookEntry> {
        return try await api.entries()
    }

    func sign(request: SignRequest) async throws RpcOnlyError {
        try await api.sign(request: request)
    }
}
```

`GuestbookService` is public, so its functions are public too, with no modifier. Only `private` is ever
written on a member.

`@Inject let api: GuestbookProtocol` gives the service a generated network client in the browser, and a
direct in-process call to `GuestbookApi` during server rendering. Injection is resolved at compile
time: a missing implementation or a dependency cycle is a compile error.

## The page

```k
// src/guestbook/Guestbook.k
~import k.validation.Form
~import k.validation.ValidationError

/// The guestbook: every entry, and a form to add one.
public page Guestbook at "/" {
    @Inject let guestbook: GuestbookService

    @Load let entries: List<GuestbookEntry> = try await guestbook.entries()

    @Form var draft: SignRequest = .empty
    @State var problem: String? = nil

    func sign() async throws ValidationError {
        do {
            try await guestbook.sign(request: draft.value)

            reload()
        } catch .rpcFailure(.invalid(let failure)) {
            throw failure
        } catch .rpcFailure {
            problem = "We couldn't save your entry. Try again."
        }
    }

    metadata {
        title("Guestbook")
    }

    view {
        main {
            heading(level: 1) { "Guestbook" }

            if entries.isEmpty {
                paragraph { "Nobody has signed yet. Be the first!" }
                    .class(.muted)
            } else {
                list {
                    for entry in entries key entry.id {
                        item {
                            EntryCard(entry: entry)
                        }
                    }
                }
            }

            form {
                textField(value: bind draft.name)
                    .label("Name")

                textArea(value: bind draft.message)
                    .label("Message")

                if let problem {
                    paragraph { problem }
                        .role(.alert)
                }

                button { "Sign the guestbook" }
                    .type(.submit)
                    .disabled(draft.isSubmitting)
            }
            .on(submit: draft.handle(sign))
        }
        .class(.page)
    }

    loading {
        paragraph { "Loading entries…" }
            .class(.muted)
    }

    failed {
        paragraph { "The guestbook couldn't be loaded." }

        button { "Try again" }
            .on(click: reload)
    }

    style {
        .page {
            maxWidth: 40rem;
            margin: 0 auto;
            padding: .lg;
        }

        .muted { color: .muted; }
    }
}
```

A lot happens in very little code:

- **`@Load`** fetches data before the view renders. Inside `view`, `entries` is a plain, already-loaded
  list: no `isLoading` flags and no `if let`. The `loading` block is required, so a loading state is
  never forgotten. If the load throws, `failed` renders instead.
- **Server rendering** awaits the load on the server and sends the result with the HTML, so the
  browser doesn't fetch it again.
- **`@Form`** wraps `SignRequest` with per-field state. `.empty`, generated for every `Validatable`
  struct used with `@Form`, starts every field blank. Binding a field with `bind draft.name` wires up
  its error message and its accessibility attributes automatically. `draft.handle(sign)` validates
  first, sets `isSubmitting`, and prevents double submits. `draft.value`, the parsed request, can be
  read only inside the action passed to `handle`, because only there has validation passed.
- **`catch` is exhaustive.** `sign` rethrows the server's `ValidationError`, which the form shows under
  the fields it names, and records any other failure in `problem`. The handler `draft.handle` returns
  doesn't throw, as every event handler must.
- **`reload()`** re-runs the page's loads. The view keeps showing the old entries until the new ones
  arrive, then swaps them in at once.

The page uses a small component for each entry:

```k
// src/guestbook/Guestbook.k (a helper in the same file)
component EntryCard {
    @Prop let entry: GuestbookEntry

    view {
        article {
            heading(level: 2) { entry.name }

            paragraph { entry.message }
        }
        .class(.card)
    }

    style {
        .card {
            padding: .md;
            borderRadius: .md;
            backgroundColor: .surface;
        }
    }
}
```

`EntryCard` has no modifier, so it is visible only within the `guestbook` module. Its `.card` class is
scoped: another component's `.card` can't collide with it.

## A test

Tests live next to the code. With no fakes declared, a test group gets the **real stack in-process**:
the real service, the protocol called through the real wire format, and a fresh in-memory SQLite
database for each test, with every migration applied.

```k
// src/guestbook/GuestbookService.test.k
test GuestbookService {
    @Inject let guestbook: GuestbookService

    test "a signed entry is listed" {
        try await guestbook.sign(request: SignRequest(name: "Ana", message: "Hello!"))

        let entries = try await guestbook.entries()

        expect entries.count == 1
    }
}
```

`expect` takes any boolean expression. When it fails, the test runner shows the value of each part, for
example ``note: `entries.count` was 0``.

## Running it

There is nothing to set up. Without a `DATABASE_URL`, `k dev` keeps the development database in a
SQLite file below the project's `.k/` directory (`.k/<package name>.db`), and creates it on the first
start. To use another database, set `DATABASE_URL` in the environment or in a `.env` file.

```bash
k dev       # creates the database, applies the migration and serves the app
k test      # runs the test against an in-memory database
k check     # types, every static rule, the lockfiles and the tests, as CI runs it
```

## What the compiler checked for you

Along the way the compiler verified, without running anything:

- that the page can't reach the database directly, and that nothing server-only ended up in the
  browser bundle;
- that `GuestbookApi` implements every rpc of `GuestbookProtocol` with matching types;
- that the repository query translates to SQL, and that `schema.lock` matches the entity;
- that every class used in a view exists in the `style` block, and every token used in a style exists in
  the theme;
- that the form fields bind to fields that exist, with matching types;
- that the page's URL is recorded in `routes.lock`, so renaming it later can't break bookmarks
  silently.

The rest of this book explains each of these pieces in depth, starting with
[the basics](language/basics.md).
