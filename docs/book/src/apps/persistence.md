# Persistence

Database access uses **entities and repositories**, the structure of Spring Data, with **LINQ-style
queries**: ordinary collection lambdas that the compiler translates to SQL.

```k
~import k.db.Database
~import k.text.EmailAddress
~import k.db.Id
~import k.db.Relation
~import k.db.Unique
~import k.crypto.Uuid
~
~entity Order {
~    @Id let id: Uuid
~    @Relation var user: User
~}
~
entity User {
    @Id let id: Uuid
    @Unique var email: EmailAddress
    var name: String
    var lastSeen: Date
    @Relation var orders: List<Order> = []
}

repository UserRepository for User {
    func byEmail(email: EmailAddress) async -> User? {
        return await users.first { user => user.email == email }
    }

    func activeSince(date: Date) async -> List<User> {
        return await users
            .filter { user => user.lastSeen > date }
            .sortedBy { user => user.name }
            .limit(50)
    }
}

service AccountService {
    @Inject let users: UserRepository
    @Inject let database: Database
}
```

**Postgres in production; SQLite for local development and tests.** The connection comes from the
standard `DatabaseConfig`: `DATABASE_URL`, such as `postgres://shop@db:5432/shop` in production, and an
optional `DATABASE_PASSWORD` secret. In development it defaults to a SQLite file, `.k/<package>.db`, which
`k dev` creates and migrates at startup.

## Entities and tables

An `entity` declares a table. Its fields are columns, annotated where they need more than a type:

| Annotation | Meaning |
|---|---|
| `@Id` | the primary key |
| `@Unique` | a unique index; makes `insert` and `update` throw on a duplicate |
| `@Index` | a single-column index |
| `@Relation` | a relation to another entity |
| `@Version` | an optimistic-locking field: `@Version var revision: Int = 1` |

- **A table is named after its entity, in snake case:** `entity Note` → `note`, `entity OrderLine` →
  `order_line`. **A column is named after its field:** `isPinned` → `is_pinned`. Generated SQL always
  quotes identifiers, so an entity named `Order` or `User` works on both databases.
- **The names are recorded in `schema.lock`** the first time a migration creates them, and then kept.
  Renaming an entity or a field keeps its table or column, so a rename never becomes a drop and a create
  (see [Renames, and rolling deploys](#renames-and-rolling-deploys)).
- **A to-one `@Relation` stores a foreign key** named after the field with `_id`: `@Relation let note:
  Note` → `note_id`. Foreign keys are always indexed.
- **A to-many `@Relation`** (`@Relation var tags: List<Tag> = []`) has no column. It is the other side of
  exactly one to-one relation in the target entity, and must default to `[]`.
- **Enum fields** without payloads are stored as their case names, and `schema.lock` records the case
  list. **`Money` fields** are one logical column stored as two, `<name>_amount` and `<name>_currency`.
- **Entities are constructed like structs,** and have value semantics: `var edited = current` followed by
  assignments produces a changed copy.
- The `k_` table prefix is reserved for the standard library (`k_session`, `k_job`, `k_rate_limit`).

**Entities and repositories are server-only.** Only server-side services may inject repositories, and
the compiler keeps both out of the browser bundle. To send entity data to the browser, map it to a
`Codable` struct in the service that implements your protocol.

## Queries are collection code

Inside `repository UserRepository for User`, the name `users`, the entity's plural in lower camel case,
is the table as a query collection. It is visible only inside the repository. You query it with the same
[collection API](../language/collections.md) you use everywhere else:

- **The compiler translates the lambdas to SQL at compile time.** A query that can't be translated, for
  example one that calls an arbitrary function inside `filter`, is a **compile error**, never a silent
  in-memory scan.
- **There are no queries derived from method names**, like `findByEmailAndStatusOrderByCreatedAtDesc`.
  Lambdas are the one way to write queries.

| Call | SQL |
|---|---|
| `filter { … }` | `WHERE …` |
| `first { … }` | `… LIMIT 1`, returning an optional |
| `single { … }` | exactly one row; **panics** on zero or several, for rows the caller has just written |
| `sortedBy`, `sortedByDescending`, then `thenBy`, `thenByDescending` | `ORDER BY a, b DESC` |
| `limit(n)` | `LIMIT n` |
| `including { x => x.relation }` | loads a relation for every row, with one extra query per relation |

### A query runs when it is awaited

**A query is a description until it is awaited.** `notes.filter { … }.sortedBy { … }` builds SQL; nothing
is sent until the expression is awaited. **Every query is awaited explicitly**, including one that is
returned: `return await users.first { … }`. A query that is returned, assigned or passed without `await`
is a compile error, with a quick fix that inserts it. So everything awaited on a collection is SQL, and
every suspension point is visible.

### Relations load only when asked

```k
// examples/shop/shop/src/store/OrderRepository.k
func byId(_ id: Uuid) async -> Order? {
    return await orders
        .including { order => order.product }
        .first { order => order.id == id }
}
```

Reading a relation that wasn't loaded is a compile error where the compiler can tell, and a panic
otherwise. N+1 queries can't happen by accident.

## Writes

Every repository inherits these, callable from inside it and by the services that inject it:

| Method | Meaning |
|---|---|
| `insert(entity)` | inserts one row |
| `insert(all: entities)` | inserts many rows in one statement |
| `update(entity)` | writes every column of the row with the entity's id; relations aren't written |
| `delete(entity)` | deletes the row with the entity's id |

- **Unique fields make `insert` and `update` throw.** On an entity with `@Unique` fields, they declare
  `throws Account.UniqueViolation`, an enum the compiler generates with one case per unique field. A
  duplicate is a normal outcome ("email already taken"), so it is typed instead of a racy pre-check:

  ```k
  // examples/notes/src/accounts/AuthApi.k
  do {
      try await accounts.insert(account)
  } catch .email {
      throw SignUpError.emailTaken
  }
  ```

  On entities without unique fields, writes don't throw.
- **`@Version` makes `update` conditional.** On an entity with a `@Version` field, `update(entity)` runs
  `UPDATE … WHERE id = ? AND revision = ?`, increments the field and returns a `Bool`. `false` means the
  row changed since it was read, or no longer exists, so a lost update can't happen silently:

  ```k
  // examples/notes/src/notes/NotesApi.k
  var edited = current
  edited.title = draft.title.trimmed()
  edited.body = draft.body
  edited.updatedAt = clock.now

  // The revision the edit started from. `update` writes only if the
  // stored revision still equals it, and then increments it.
  edited.revision = revision

  let saved = await notes.update(edited)

  guard saved else {
      guard let latest = await notes.find(id: id, ownerId: ownerId) else {
          throw SaveError.notFound
      }

      throw SaveError.conflict(latest: summarize(note: latest))
  }
  ```

- **Writes don't touch relations.** `update(note)` writes `note`'s own columns, never its `tags`. Related
  rows are written through their own repository, so every write is visible at its call site.
- **Infrastructure failures panic.** A lost connection or a full disk isn't something the calling code
  can handle, so writes don't declare it.
- A repository can define its own write functions on top of these, but can't redefine an inherited name.

### Bulk writes on queries

A filtered query can be updated or deleted without loading rows. Both return the number of rows affected:

```k
// examples/shop/shop/src/store/OrderRepository.k
let updated = await orders
    // ...
    .filter { order => order.id == id and order.status == from }
    .update { order => order.status = to }
```

```k
// examples/notes/src/notes/TagRepository.k
await tags
    .filter { tag => tag.note.id == note.id }
    .delete()
```

- The lambda passed to `update` may only assign fields, with values that translate to SQL.
- A bulk update writes only the fields it assigns. **Prefer it for single-field changes**, such as
  pinning a note or moving an order's status: it can't overwrite a concurrent edit of other fields, as
  `update(entity)` from an old copy would.

## `Database` and transactions

`Database`, from `k.db`, is injectable wherever a repository is.

```k
// examples/shop/shop/src/store/PaymentRecorder.k
let recorded = await database.transaction {
    let marked = await orders.markPaid(id: order.id, reference: reference, paidAt: clock.now)

    if marked {
        await jobs.enqueue(OrderConfirmationJob.kind, payload: OrderConfirmation(orderId: order.id))
    }

    marked
}
```

- **`database.transaction { … }`** runs the block in one transaction and returns its value. It is
  [error-generic](../language/errors.md#functions-that-pass-errors-through): the call throws whatever the
  block throws, rolling back first, and a block that can't throw makes the call non-throwing.
- Calls to every repository inside the block share the transaction. Writes outside a transaction commit
  statement by statement.
- **A nested `transaction` is a savepoint.** A throw from the inner block rolls back to the savepoint and
  rethrows.
- **Commit hooks fire when the outermost transaction commits,** and are discarded when it rolls back:
  enqueued [jobs](jobs.md) become visible to the worker, and [`Broadcast`](streaming.md) messages are
  published. So "mark the order paid" and "send the confirmation" can't diverge.
- **Isolation** is `READ COMMITTED` on Postgres; SQLite serializes write transactions. A read-then-write
  that must not race uses a `@Version` field or a conditional bulk update, not a read inside a
  transaction.
- `database.ping(timeout:)` runs `SELECT 1` and returns a `Bool`, for [health checks](endpoints.md#health-checks).

## Tests and the database

Every `k test` run uses a fresh in-memory SQLite database with every migration applied. **Each test runs
inside a transaction the app can't see,** rolled back afterwards. The app's outermost transaction becomes
a savepoint inside it, and its commit fires the commit hooks, exactly when they would fire in production.

## Migrations

Migrations live in **`migrations/`** at the project root, one per file, named `NNNN_description.k`. Each
file holds one `migration` declaration:

```k
// examples/notes/migrations/0002_add_note_pinning.k
// Generated by `k migrate new "add note pinning"` after `isPinned` was added
// to `notes.Note`, then reviewed and committed. Format:
// docs/design/19-persistence.md.

/// Adds pinning to notes. Existing notes start unpinned, which is what the
/// field's default in `Note` says too.
migration AddNotePinning {
    func up(schema: Schema) {
        schema.alterTable(name: "note") { table =>
            table.addColumn(name: "is_pinned", type: .boolean)
                .defaultValue(.boolean(false))
        }
    }
}
```

- **`k migrate new "description"`** writes the next file from the difference between the entities and
  `schema.lock`, and updates the lock. `k migrate new --empty "description"` writes an empty migration
  for data changes, or for a change you write by hand. Review the file and commit both.
- **`k migrate new` only writes what is safe without reviewing data:** new tables, new optional or
  defaulted columns, new indexes, and the retirement of a removed field. It refuses, explaining each
  change and writing nothing, a required column added to an existing table without a default, a changed
  column type or option, an added or removed enum case, and a removed entity. Write those in a `--empty`
  migration; `k build` then records what it does in `schema.lock`.
- **The order comes from the `migrations` list in `schema.lock`.** File numbers must match their position
  in it, so two branches that both add `0003_…` conflict visibly in review.
- **`migrations/` is not a module.** Migration files can't import anything, and nothing can import them.
- **Migrations name tables and columns as strings,** not as entity references, because a migration is a
  record of history and must keep its meaning after `Note` is renamed. Every name is still checked, by
  replaying the migrations, so a typo is a compile error.
- **There is no `down`.** A mistake is fixed by a new migration. Each migration runs in one transaction.
- `dist/app serve` applies pending migrations at startup.

### The `Schema` builder

| Call | Meaning |
|---|---|
| `schema.createTable(name:) { table => … }` | creates a table; columns with `table.column(name:type:)` |
| `schema.alterTable(name:) { table => … }` | `addColumn(name:type:)`, `renameColumn(from:to:)`, `dropColumn(name:)`, `retireColumn(name:)`, `addEnumCase(column:case:)`, `renameEnumCase(column:from:to:)`, `removeEnumCase(column:case:replacement:)` |
| `schema.renameTable(from:to:)`, `schema.dropTable(name:)`, `schema.retireTable(name:)` | renames, drops or retires a table |
| `schema.createIndex(table:columns:)` | an index over one or more columns |
| `schema.insert(into:rows:)` | inserts data rows, each a `Map<String, ColumnValue>` |
| `schema.execute(postgres:sqlite:)` | raw SQL, written for both databases |

```k
// examples/shop/shop/migrations/0001_create_catalog_and_orders.k
migration CreateCatalogAndOrders {
    func up(schema: Schema) {
        schema.createTable(name: "product") { table =>
            table.column(name: "id", type: .uuid).primaryKey(true)
            table.column(name: "slug", type: .text).unique(true)
            // ...
            table.column(name: "price", type: .money)
            // ...
        }

        schema.createTable(name: "order") { table =>
            table.column(name: "id", type: .uuid).primaryKey(true)
            table.column(name: "product_id", type: .uuid).references(table: "product")
            // ...
            table.column(name: "status", type: .enumeration(cases: ["awaitingPayment", "paid", "needsReview", "cancelled"]))
                .defaultValue(.text("awaitingPayment"))
            table.column(name: "payment_reference", type: .text).nullable(true)
            // ...
        }
    }
}
```

- **Column options are chained, and boolean options take their value:** `.primaryKey(true)`,
  `.unique(true)`, `.nullable(true)`, `.indexed(true)`, `.references(table:)`, `.defaultValue(…)`.
- **`.references(table:)` implies an index,** matching the entity rule for foreign keys.
- **Column types** are a closed enum mapped per database: `.uuid`, `.text`, `.integer`, `.decimal`,
  `.boolean`, `.timestamp`, `.money`, `.json`, `.enumeration(cases:)`. A `.money` column is one logical
  column; the builder expands it to its two halves for every operation.
- **A column added to an existing table must be nullable or have a default,** because existing rows need
  a value.
- **Destructive changes** (dropping a column or table, renaming one, removing or renaming an enum case)
  require `@Breaking("reason")` on the migration, and `k check` fails without it. Adding an enum case
  doesn't. Narrowing a type is written with `execute`, which the compiler can't see into, so mark it
  yourself.
- **The enum case operations rewrite the stored values.** `removeEnumCase` moves the rows holding the
  removed case to `replacement`, another of the column's cases, and `renameEnumCase` moves them to the
  new name; a column default of the old case moves with it. On SQLite, which can't change a constraint,
  the table is rebuilt with its rows and every index the migrations created; an index or constraint you
  added with `execute` has to be added again after such a migration.

```k
// migrations/0003_rename_order_statuses.k (written by hand after `k migrate new --empty`)
/// Orders that were `needsReview` are `onHold` now, and `lost` ones are cancelled.
@Breaking("support confirmed no client shows `lost`")
migration RenameOrderStatuses {
    func up(schema: Schema) {
        schema.alterTable(name: "order") { table =>
            table.renameEnumCase(column: "status", from: "needsReview", to: "onHold")
            table.removeEnumCase(column: "status", case: "lost", replacement: "cancelled")
        }
    }
}
```

### Renames, and rolling deploys

- **Renaming an entity or a field changes no names in the database.** `schema.lock` maps the new name to
  the old table or column, so no migration is needed and no data moves. Record the rename with
  `k migrate rename`, which changes only the lock:

  ```sh
  k migrate rename notes.Note.title heading     # a field: `heading` keeps the column `note.title`
  k migrate rename notes.Note notes.Memo        # an entity: `Memo` keeps the table `note`
  ```

  Until you do, `k check` reports the renamed entity or field as a removal and an addition, and names the
  command when it looks like a rename.
- **A physical rename is written only when asked for:** `k migrate new --rename notes.Note.heading
  "rename the heading column"` renames the column so it follows the field's current name (`--rename
  notes.Memo` renames the table), and marks the migration `@Breaking("single instance")`, because the
  previous release still reads the old name.
- **During a rolling deploy, the previous release runs against the new schema,** so migrations follow
  expand, then contract. Removing a field *retires* its column: the generated migration calls
  `table.retireColumn(name:)`, which makes the column nullable if it is `NOT NULL` without a default,
  `schema.lock` marks it `retired`, and the app stops reading and writing it. A table is retired with
  `schema.retireTable(name:)`. A retired column or table is dropped by a later migration
  (`k migrate new --drop-retired "description"`, marked `@Breaking`), and `k check` fails on a drop or
  rename of anything still live, explaining the two-release sequence.
- **Single-instance apps can opt out** with `@Breaking("single instance")` on the migration, which allows
  dropping or renaming a live column in one step.

### Data migrations

Data migrations run in every environment, tests included. They are for reference data every environment
needs, such as countries or plans, so every test sees those rows. **Development-only seed data doesn't
belong in a migration,** because it would ship to production.

### `schema.lock`

`schema.lock` is TOML, generated and committed. It records the schema the migrations produce, so the
compiler can compare it with the entities without a database:

```toml
# Generated by `k migrate`. Do not edit by hand.
migrations = ["0001_create_catalog_and_orders", "0002_add_order_review"]

[tables.order]
entity = "store.Order"
columns = [
    { field = "id", name = "id", type = "uuid", primaryKey = true },
    { field = "product", name = "product_id", type = "uuid", references = "product.id", indexed = true },
    { field = "status", name = "status", type = "enumeration", cases = ["awaitingPayment", "paid", "needsReview", "cancelled"], default = "awaitingPayment" },
    { field = "total", name = "total", type = "money" },
    { name = "coupon_code", type = "text", nullable = true, retired = true },
]
```

- Each column records the field it belongs to and its locked name. A retired column has no field; a
  retired table has `retired = true`.
- `k check` fails when the entities differ from the lock ("run `k migrate new`"), when replaying
  `migrations/` doesn't produce the lock, when file numbers don't match the `migrations` list, when a
  migration doesn't replay, and when a destructive migration lacks its `@Breaking`. It compares without a
  database, for an app whose program has no other errors.
- `k migrate new` and `k migrate rename` write the lock with their change. `k build` records what the
  migrations produce, so a migration you wrote or edited by hand gets into the lock, keeping the entity
  and field names. `k check` never writes it, so CI catches a lock nobody rebuilt.
- The build (`k build`, `k run`, `k dev`, `k test`) replays `migrations/` in the lock's order, and fails on
  a migration that doesn't replay or lacks its `@Breaking`, and when an entity's table or a stored column
  isn't what the migrations create.
- Tables managed by the standard library (`k_session`, `k_job`, `k_rate_limit`) aren't recorded; the
  standard library migrates them itself.
