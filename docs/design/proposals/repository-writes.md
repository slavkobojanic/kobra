# Proposal: Repository Writes and Query Execution

> **Status: proposal.** Not yet accepted into the design. First used by `examples/notes`, and consistent with the repositories in `examples/shop`.

## Problem

[19-persistence.md](../19-persistence.md) shows repository *queries* (`users.first { … }`, `.filter`, `.sortedBy`, `.limit`, `.including`) and transactions (`try await db.transaction { … }`), but not how rows are inserted, updated or deleted, where `db` comes from, or when a query actually runs.

## Proposal

### The collection

Inside `repository NoteRepository for Note`, the name **`notes`** (the entity's plural, in lower camel case) is the table as a query collection, as in [19-persistence.md](../19-persistence.md). It is visible only inside the repository.

### When a query runs

- **A query is a description until it is awaited.** `notes.filter { … }.sortedBy { … }` builds SQL; nothing is sent until it's awaited.
- **Returning a query from a repository function runs it.** A function declared `async -> List<Note>` or `async -> Note?` may `return notes.filter { … }`, as in [19-persistence.md](../19-persistence.md); the compiler awaits the query as part of the return. This is the only place a query runs without a written `await`.
- **Everything else is awaited explicitly**, including writes used as statements: `await notes.filter { … }.delete()`.

### Inherited writes

Every repository inherits these, callable from inside it and by the services that inject it:

| Method | Meaning |
|---|---|
| `insert(entity)` | inserts one row |
| `insert(all: entities)` | inserts many rows in one statement |
| `update(entity)` | writes every column of the row with the entity's id. Relations aren't written. |
| `delete(entity)` | deletes the row with the entity's id |

- **Writes don't touch relations.** `update(note)` writes `note`'s own columns, never its `tags`. Related rows are written through their own repository, which keeps every write visible at its call site.
- A repository can define its own write functions on top of these, for example one that deletes a note together with its tags. It can't redefine an inherited name.

### Bulk writes on queries

A filtered query can be updated or deleted without loading rows. Both return the number of rows affected:

```k
let shipped = await orders
    .filter { order => order.id == id }
    .filter { order => order.status == .paid }
    .update { order => order.status = .shipped }

await tags
    .filter { tag => tag.note.id == note.id }
    .delete()
```

The lambda passed to `update` may only assign fields, and each assigned value must be translatable to SQL, like every query lambda.

### Reading

In addition to the collection API in [08-types.md](../08-types.md), these are translated to SQL:

| Call | SQL |
|---|---|
| `first { … }` | `… LIMIT 1`, returns `T?` |
| `single { … }` | exactly one row, returns `T`; **panics** on zero or several, for rows the caller has just written |
| `sortedBy` / `sortedByDescending`, then `thenBy` / `thenByDescending` | `ORDER BY a, b DESC` |
| `limit(n)` | `LIMIT n` |
| `including { x => x.relation }` | loads a relation for every row, with one extra query per relation, never one per row |

### Entity construction

- An entity is constructed like a struct, with ordinary default parameters: `Note(id: Uuid.random(), ownerId: owner, title: "Plan", body: "")`, where `isPinned`, `revision` and the timestamps have defaults.
- **A to-many relation field must default to `[]`.** A freshly constructed entity then has its relation loaded and empty, so reading it is never the "unloaded relation" panic of [19-persistence.md](../19-persistence.md).
- Entities have value semantics, like structs. `var edited = current` followed by field assignments produces a changed copy, which `update(edited)` writes.

### `Database` and transactions

- **`Database` is a stdlib service in `k.db`,** injectable wherever a repository is (server-side services). Its main member is `transaction`.
- `try await database.transaction { … }` runs the block in one transaction and **rethrows the block's error type**, rolling back on a throw; with a block that can't throw, it's `await database.transaction { … }`. The block's return value is returned.
- Calls to every repository inside the block share the transaction. Nested `transaction` calls join the outer one.
- The connection comes from the stdlib's `DatabaseConfig` (`DATABASE_URL`, such as `sqlite:.k/notes.db` or `postgres://…`; see [config-and-secrets.md](config-and-secrets.md)). Every `k test` run uses a fresh in-memory SQLite database instead, with each test in a transaction that is rolled back ([21-testing.md](../21-testing.md)).

## Why

- Built-in writes keep repositories focused on queries, the one thing that differs between them, as Spring Data does with `CrudRepository`.
- Making the query run point explicit (await or return) keeps "a query that can't be translated is a compile error" true: everything awaited is SQL.
- Not writing relations implicitly avoids ORMs' cascading-save surprises, at the cost of one explicit call.
