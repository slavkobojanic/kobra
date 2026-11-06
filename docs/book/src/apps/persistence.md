# Persistence

Database access uses **entities and repositories**, the structure of Spring Data, with **LINQ-style
queries**: ordinary collection lambdas that the compiler translates to SQL.

```k
entity User {
    @Id let id: UserId
    @Unique var email: String
    var name: String
    var lastSeen: Date
    @Relation var orders: List<Order> = []
}

repository UserRepository for User {
    func byEmail(email: String) async -> User? {
        return users.first { user => user.email == email }
    }

    func activeSince(date: Date) async -> List<User> {
        return users
            .filter { user => user.lastSeen > date }
            .sortedBy { user => user.name }
            .limit(50)
    }
}

service AccountService {
    @Inject let users: UserRepository
}
```

## Entities

An `entity` declares a table. Its fields are columns, annotated where they need more than a type:

| Annotation | Meaning |
|---|---|
| `@Id` | the primary key |
| `@Unique` | a unique constraint |
| `@Index` | a single-column index |
| `@Relation` | a relation to another entity |

**Entities and repositories are server-only.** Only server-side services may inject repositories, and
the compiler keeps both out of the browser bundle. To send entity data to the browser, map it to a
`Codable` struct in the service that implements your protocol.

> **Proposal.** Table and column naming, entity construction and writes follow
> `docs/design/proposals/migrations.md` and `docs/design/proposals/repository-writes.md`. For example,
> `entity OrderLine` is stored in a table `order_line`, and a field `isPinned` in a column `is_pinned`.

## Queries are collection code

Inside `repository UserRepository for User`, the name `users`, the entity's plural, is the table as a
query collection. You query it with the same [collection API](../language/collections.md) you use
everywhere else:

- **The compiler translates the lambdas to SQL at compile time.** A query that can't be translated, for
  example one that calls an arbitrary function inside `filter`, is a **compile error**, never a silent
  in-memory scan.
- **There are no queries derived from method names**, like `findByEmailAndStatusOrderByCreatedAtDesc`.
  Lambdas are the one way to write queries.

| Call | SQL |
|---|---|
| `filter { … }` | `WHERE …` |
| `first { … }` | `… LIMIT 1`, returning an optional |
| `sortedBy`, `sortedByDescending`, then `thenBy`, `thenByDescending` | `ORDER BY …` |
| `limit(n)` | `LIMIT n` |
| `including { x => x.relation }` | loads a relation, with one extra query per relation |

A query is a description until it is awaited, or returned from a repository function.

## Relations load only when asked

**Relations load only when requested** with `.including`:

```k
func withOrders(id: UserId) async -> User? {
    return users
        .including { user => user.orders }
        .first { user => user.id == id }
}
```

Reading a relation that wasn't loaded is a compile error where the compiler can tell, and a panic
otherwise. N+1 queries can't happen by accident.

## Writes

Every repository has `insert`, `insert(all:)`, `update` and `delete`, and filtered queries can be updated
or deleted in bulk:

```k
await users.insert(user)

let shipped = await orders
    .filter { order => order.status == .paid }
    .update { order => order.status = .shipped }
```

Writes never touch relations implicitly. Related rows are written through their own repository, so every
write is visible at its call site.

## Transactions

```k
try await database.transaction {
    try await accounts.debit(from, amount)
    try await accounts.credit(to, amount)
}
```

The block runs in one transaction, which **rolls back if it throws**. Every repository called inside it
shares the transaction.

## Databases

**Postgres in production; SQLite for local development and tests.** The connection comes from the
`DATABASE_URL` environment variable, for example `sqlite:.k/shop.db` in your `.env`, or
`postgres://shop@db:5432/shop` in production. Every `k test` run uses a fresh in-memory SQLite database,
and each test is rolled back.

## Migrations and `schema.lock`

- `schema.lock` records the current database schema, and is committed.
- **`k migrate new "description"`** generates a reviewable migration from the difference between your
  entities and the lock, and updates the lock.
- **Destructive changes**, such as dropping a column or narrowing a type, require `@Breaking("reason")`.
- `k check` fails when the entities and the lock disagree.

> **Proposal.** The migration file format follows `docs/design/proposals/migrations.md`.

Migrations live in `migrations/`, one `migration` declaration per numbered file:

```k
// migrations/0002_add_note_pinning.k

/// Adds pinning. Existing notes start unpinned.
migration AddNotePinning {
    func up(schema: Schema) {
        schema.alterTable(name: "note") { table =>
            table.addColumn(name: "is_pinned", type: .boolean)
                .defaultValue(.boolean(false))
        }
    }
}
```

- Migrations name tables and columns as strings, because a migration is a record of history that must
  keep its meaning after an entity is renamed. Replaying the migrations still checks every name.
- There is no `down`; a mistake is fixed by a new migration.
- Each migration runs in its own transaction, and the server applies pending migrations at startup.
