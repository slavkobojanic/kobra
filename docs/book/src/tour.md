# A tour of K

This chapter builds a small full-stack feature, a guestbook, from the database table to the button.
It moves quickly and skips details: the point is to show how the pieces of a K app fit together. Every
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

/// What a visitor fills in to sign the guestbook.
public struct SignRequest implements Codable, Validatable {
    @NotBlank @Length(max: 40) var name: String
    @NotBlank @Length(max: 500) var message: String

    /// An empty form.
    static let empty = SignRequest(name: "", message: "")
}
```

## The database

An `entity` describes a table, and a `repository` holds its queries. Queries are ordinary collection
code, and the compiler translates the lambdas into SQL at compile time:

```k
// src/guestbook/Entry.k
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
        return entries
            .sortedBy { entry => entry.createdAt }
            .limit(100)
    }
}
```

Inside the repository, `entries` is the table. A query that can't be translated to SQL is a compile
error, never a silent in-memory scan. `entity` and `repository` are **server-only**: the compiler keeps
them, and anything that touches them, out of the browser bundle.

Generate the migration that creates the table:

```bash
k migrate new "create entries"
```

The toolchain compares your entities with `schema.lock` and writes
`migrations/0001_create_entries.k` for you to review and commit.

## The contract

A `protocol` declares what the browser can ask the server. It compiles to Protobuf and runs over the
Connect protocol, so you never write a URL, a fetch call or a JSON parser:

```k
// src/guestbook/GuestbookProtocol.k

/// What the browser can ask the guestbook server.
public protocol GuestbookProtocol {
    rpc entries() -> List<GuestbookEntry>

    rpc sign(request: SignRequest) -> GuestbookEntry
}
```

## The server

A service that `implements` a protocol is its server implementation. It may inject repositories,
because it is server-only:

```k
// src/guestbook/GuestbookApi.k
service GuestbookApi implements GuestbookProtocol {
    @Inject let repository: EntryRepository

    rpc entries() async -> List<GuestbookEntry> {
        let latest = await repository.latest()

        return latest.map(summarize)
    }

    rpc sign(request: SignRequest) async -> GuestbookEntry {
        let entry = Entry(id: Uuid.random(), name: request.name, message: request.message, createdAt: Date.now)

        await repository.insert(entry)

        return summarize(entry: entry)
    }

    private func summarize(entry: Entry) -> GuestbookEntry {
        return GuestbookEntry(id: entry.id, name: entry.name, message: entry.message)
    }
}
```

`SignRequest` is `Validatable`, so the runtime validates every request **before** `sign` runs. A
client that skips the form can't skip the rules.

## The client service

Components never call protocols directly. Remote calls go through a service, which is where caching,
retries and shared state belong. This one is thin for now:

```k
// src/guestbook/GuestbookService.k
public service GuestbookService {
    @Inject let api: GuestbookProtocol

    public func entries() async -> List<GuestbookEntry> {
        return await api.entries()
    }

    public func sign(request: SignRequest) async {
        await api.sign(request: request)
    }
}
```

`@Inject let api: GuestbookProtocol` gives the service a generated network client in the browser, and a
direct in-process call to `GuestbookApi` during server rendering. Injection is resolved at compile
time: a missing implementation or a dependency cycle is a compile error.

## The page

```k
// src/guestbook/Guestbook.k

/// The guestbook: every entry, and a form to add one.
public page Guestbook at "/" {
    @Inject let guestbook: GuestbookService

    @Load let entries: List<GuestbookEntry> = await guestbook.entries()

    @Form var draft: SignRequest = .empty

    func sign() async {
        await guestbook.sign(request: draft.value)

        reload()
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
  never forgotten.
- **Server rendering** awaits the load on the server and sends the result with the HTML, so the
  browser doesn't fetch it again.
- **`@Form`** wraps `SignRequest` with per-field state. Binding a field with `bind draft.name` wires up
  its error message and its accessibility attributes automatically. `draft.handle(sign)` validates
  first, sets `isSubmitting`, and prevents double submits.
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
the real service, the protocol called directly, and a fresh SQLite database that is rolled back after
each test.

```k
// src/guestbook/GuestbookService.test.k
test GuestbookService {
    @Inject let guestbook: GuestbookService

    test "a signed entry is listed" {
        await guestbook.sign(request: SignRequest(name: "Ana", message: "Hello!"))

        let entries = await guestbook.entries()

        expect entries.count == 1
    }
}
```

`expect` takes any boolean expression. When it fails, the test runner shows the value of each part, for
example ``entries.count was 0, expected == 1``.

## Running it

Point `k dev` at a local database by creating `.env`:

```bash
DATABASE_URL=sqlite:.k/guestbook.db
```

Then:

```bash
k dev       # applies the migration and serves the app
k test      # runs the test against an in-memory database
k check     # everything, as CI runs it
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
