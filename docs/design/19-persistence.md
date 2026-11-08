# Persistence

## Decision

Database access uses **entities and repositories** (Spring Data's structure), with **LINQ-style queries**: ordinary collection lambdas that the compiler translates to SQL.

```k
entity User {
    @Id let id: UserId
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

## Rules

- **`entity` and `repository` are declaration keywords. Both are server-only.** Only server-side services may inject repositories (see [10-services.md](10-services.md)).
- **Queries use the standard collection API.** The compiler translates the lambdas to SQL at compile time. A query that can't be translated (for example, calling an arbitrary function inside `filter`) is a **compile error**, never a silent in-memory scan.
- **There are no queries derived from method names** like `findByEmailAndStatusOrderByCreatedAtDesc`. Lambdas are the one way to write queries.
- **Relations load only when requested:** `.including { user => user.orders }`. Reading an unloaded relation is a compile error where the compiler can tell, and a panic otherwise. This prevents N+1 queries by default.
- **Databases:** Postgres in production; SQLite for local development and tests.

## Entities and tables

- **A table is named after its entity, in snake case:** `entity Note` → `note`, `entity OrderLine` → `order_line`. **A column is named after its field:** `isPinned` → `is_pinned`.
- **The names are recorded in `schema.lock`** the first time a migration creates them, and then kept. Renaming an entity or a field keeps its table or column, so a rename never turns into a drop and a create ([Renames](#renames)).
- **Generated SQL always quotes identifiers,** so entities named after reserved words (`Order` → `"order"`, `User` → `"user"`) work on both databases.
- **The `k_` table prefix is reserved for the stdlib** (`k_session`, `k_job`, `k_rate_limit`). An entity that would map to it is a compile error.
- **A to-one `@Relation` field stores a foreign key** named after the field with `_id`: `@Relation let note: Note` → `note_id`, referencing `note.id`. Foreign keys are always indexed.
- **A to-many `@Relation` field** (`@Relation var tags: List<Tag> = []`) has no column. It is the other side of exactly one to-one relation in the target entity (`Tag.note`); none, or more than one, is a compile error asking for `@Relation(inverse: …)`. It must default to `[]`, so a freshly constructed entity has its relation loaded and empty.
- **`@Index`** on a field adds a single-column index; **`@Unique`** adds a unique index. Multi-column indexes are written in a migration.
- **`@Version var revision: Int = 1`** marks an optimistic-locking field ([Writes](#writes)).
- **Enum fields** without payloads are stored as their case names. `schema.lock` records the case list, so renaming or removing a case is a breaking schema change ([Migrations](#the-schema-builder)).
- **`Money` fields** are one logical column that the database stores as two, `<name>_amount` and `<name>_currency` ([34-money.md](34-money.md)).
- **Entities are constructed like structs,** with ordinary default parameters (`Note(id: Uuid.random(), ownerId: owner, title: "Plan", body: "")`), and have value semantics: `var edited = current` followed by assignments produces a changed copy.

## Queries

### The collection

Inside `repository NoteRepository for Note`, the name **`notes`** (the entity's plural, in lower camel case) is the table as a query collection. It is visible only inside the repository.

### A query runs when it is awaited

- **A query is a description until it is awaited.** `notes.filter { … }.sortedBy { … }` builds SQL; nothing is sent until the expression is awaited.
- **Every query is awaited explicitly,** including one that is returned: `return await users.first { … }`. A query that is returned, assigned or passed without `await` is a compile error, with a quick fix that inserts it.
- So everything awaited on a collection is SQL, and every suspension point is visible.

### Reading

In addition to the collection API in [08-types.md](08-types.md), these are translated to SQL:

| Call | SQL |
|---|---|
| `first { … }` | `… LIMIT 1`, returns `T?` |
| `single { … }` | exactly one row, returns `T`; **panics** on zero or several, for rows the caller has just written |
| `sortedBy` / `sortedByDescending`, then `thenBy` / `thenByDescending` | `ORDER BY a, b DESC` |
| `limit(n)` | `LIMIT n` |
| `including { x => x.relation }` | loads a relation for every row, with one extra query per relation, never one per row |

## Writes

Every repository inherits these, callable from inside it and by the services that inject it:

| Method | Meaning |
|---|---|
| `insert(entity)` | inserts one row |
| `insert(all: entities)` | inserts many rows in one statement |
| `update(entity)` | writes every column of the row with the entity's id. Relations aren't written. |
| `delete(entity)` | deletes the row with the entity's id |

- **Unique fields make `insert` and `update` throw.** On an entity with `@Unique` fields, they declare `throws User.UniqueViolation`, an enum the compiler generates with one case per unique field (`.email`). A duplicate is a normal outcome ("email already taken"), so it is typed instead of racy pre-checks. On entities without unique fields, writes don't throw.

  ```k
  do {
      try await accounts.insert(account)
  } catch .email {
      throw SignUpError.emailTaken
  }
  ```

- **`@Version` makes `update` conditional.** On an entity with a `@Version` field, `update(entity)` runs `UPDATE … WHERE id = ? AND revision = ?`, increments the field, and returns `Bool`: `false` means the row changed since it was read (or no longer exists), so a lost update can't happen silently. Reload the entity to see the new version.
- **Writes don't touch relations.** `update(note)` writes `note`'s own columns, never its `tags`. Related rows are written through their own repository, which keeps every write visible at its call site.
- **Infrastructure failures panic.** A lost connection, a full disk or a deadlock that can't be retried is not something the calling code can handle, so writes don't declare it ([09-errors.md](09-errors.md#panics)).
- A repository can define its own write functions on top of these. It can't redefine an inherited name.

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

- The lambda passed to `update` may only assign fields, and each assigned value must be translatable to SQL.
- A bulk update writes only the fields it assigns. **Prefer it for single-field changes** (pinning a note, moving an order's status): it can't overwrite a concurrent edit of other fields, as `update(entity)` from an old copy would. It changes a `@Version` field only when it assigns it.

## `Database` and transactions

- **`Database` is a stdlib service in `k.db`,** injectable wherever a repository is. It connects with the stdlib `DatabaseConfig` (`DATABASE_URL`, `DATABASE_PASSWORD`; [25-config-and-secrets.md](25-config-and-secrets.md)). `database.ping(timeout:) async -> Bool` runs `SELECT 1` and never throws, for health checks ([27-endpoint-pages.md](27-endpoint-pages.md)).
- **`database.transaction { … }`** runs the block in one transaction and returns its value. It is error-generic ([09-errors.md](09-errors.md#error-generic-functions)): the call throws whatever the block throws, rolling back first, and a block that can't throw makes the call non-throwing.
- Calls to every repository inside the block share the transaction. Writes outside a transaction commit statement by statement.
- **The outermost transaction is the app-level transaction.** A nested `transaction` is a **savepoint**: a throw from the inner block rolls back to the savepoint and rethrows. If the outer code catches the error and carries on, the inner writes are gone and the outer ones remain.
- **Commit hooks fire when the app-level transaction commits,** and are discarded when it rolls back: enqueued jobs become visible to the worker ([30-background-jobs.md](30-background-jobs.md)) and `Broadcast` messages are published ([29-server-streaming.md](29-server-streaming.md)). So "mark the order paid" and "send the confirmation" can't diverge.
- **Isolation:** `READ COMMITTED` on Postgres. SQLite serializes write transactions. A read-then-write that must not race uses a `@Version` field or a conditional bulk update, not a read inside a transaction.
- **Tests** run against a fresh in-memory SQLite database per `k test` run. Each test runs inside a transaction the app can't see, which is rolled back afterwards: the app's outermost transaction becomes a savepoint inside it, and its commit releases the savepoint and fires the commit hooks ([21-testing.md](21-testing.md)).

## Migrations

### Migration files

Migrations live in **`migrations/`** at the project root, one per file, named `NNNN_description.k`. Each file holds one `migration` declaration:

```k
// Generated by `k migrate new "add note pinning"`, then reviewed.

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

- **`migration` is a declaration keyword,** because a migration has its own compiler semantics: migrations are replayed in order and checked against `schema.lock`.
- **The order comes from the `migrations` list in `schema.lock`.** File numbers must match their position in the list; `k check` fails otherwise, so two branches that both add `0003_…` conflict visibly in review.
- **`migrations/` is not a module.** Migration files can't import anything, and nothing can import them.
- **Migrations name tables and columns as strings, not as entity references.** A migration is a record of history: it must keep meaning what it meant after `Note` is renamed or `isPinned` is removed. Every name is still checked, by replaying the migrations, so a typo is a compile error.
- **There is no `down`.** Rolling back production schemas is rarely safe; a mistake is fixed by a new migration.
- **Each migration runs in one transaction** (Postgres and SQLite both support transactional DDL).
- `k migrate new "description"` writes the next file from the difference between the entities and `schema.lock`, and updates the lock. `k migrate new --empty "description"` writes an empty migration for data changes.
- **`k migrate new` only writes what is safe without review of data:** new tables, new optional or defaulted columns, new indexes, and the retirement of a removed field (`retireColumn`, below). It refuses, explaining each change and writing nothing, a required column added to an existing table without a default, a changed column type or option, an added or removed enum case, and a removed entity. Those are written by hand in a `--empty` migration, with `@Breaking` where the rules above require it.
- **A migration written or edited by hand is recorded in `schema.lock` by `k build`** (below), so `k check` can compare the lock with the replay.

### The `Schema` builder

| Call | Meaning |
|---|---|
| `schema.createTable(name:) { table => … }` | creates a table; columns with `table.column(name:type:)` |
| `schema.alterTable(name:) { table => … }` | changes a table: `addColumn(name:type:)`, `renameColumn(from:to:)`, `dropColumn(name:)`, `retireColumn(name:)`, `addEnumCase(column:case:)`, `renameEnumCase(column:from:to:)`, `removeEnumCase(column:case:replacement:)` |
| `schema.renameTable(from:to:)` | renames a table |
| `schema.dropTable(name:)` | drops a table |
| `schema.retireTable(name:)` | retires a table |
| `schema.createIndex(table:columns:)` | adds an index over one or more columns |
| `schema.insert(into:rows:)` | inserts data rows, each a `Map<String, ColumnValue>` |
| `schema.execute(postgres:sqlite:)` | runs raw SQL, written for both databases; both are required |

- **Column options are chained, and boolean options take their value** ([01-principles.md](01-principles.md)): `.primaryKey(true)`, `.unique(true)`, `.nullable(true)`, `.indexed(true)`, `.references(table:)`, `.defaultValue(…)`.
- **`.references(table:)` implies an index,** matching the entity rule for foreign keys.
- **Column types** are a closed enum mapped per database: `.uuid`, `.text`, `.integer`, `.decimal`, `.boolean`, `.timestamp`, `.money`, `.json`, `.enumeration(cases:)`.
- **A `.money` column is one logical column.** The builder expands it to `<name>_amount` and `<name>_currency` for every operation (`renameColumn`, `createIndex(columns: ["price"])`, `.nullable(true)`), so a migration never names the halves.
- **A column added to an existing table must be nullable or have a default,** or the migration is a compile error: existing rows need a value, and so do inserts from replicas still running the previous release.
- **Removing or renaming an enum case** rewrites the stored values (`removeEnumCase` replaces them with `replacement`, which must be another of the column's cases). A column default of the old case moves with it. Both require `@Breaking("reason")`. Adding a case doesn't.
- **The case operations rewrite the column's `CHECK` constraint.** Postgres swaps the constraint around an `UPDATE` of the stored values. SQLite can't alter a constraint, so the table is rebuilt (created anew, rows copied with the values mapped, the old table dropped and the new one renamed), and every index the migrations created is created again. Constraints or indexes added with `execute` aren't known to the replay and must be added again by hand after a rebuild.
- **Destructive changes** (`dropColumn`, `dropTable`, `renameColumn`, `renameTable`, `renameEnumCase`, `removeEnumCase`) require `@Breaking("reason")` on the migration, and a drop or rename of anything still live requires `@Breaking("single instance")` exactly ([Rolling deploys](#rolling-deploys-expand-then-contract)). Narrowing a type is written with `execute`, which the compiler can't classify, so its `@Breaking` is the author's responsibility. A missing `@Breaking` is `K6005`; a live drop or rename without `"single instance"` is `K6006`.

### Renames

- **Renaming an entity or a field changes no names in the database.** `schema.lock` maps the new name to the old table or column, so no migration is needed and no data moves.
- **The rename is recorded with `k migrate rename <old> <new>`:** `k migrate rename notes.Note notes.Memo` for an entity, `k migrate rename notes.Note.title heading` for a field. It changes only `schema.lock`. Until it is run, `k check` reports the renamed entity or field as a removal and an addition (`K8004`), and names the command when exactly one recorded field (or entity) is gone and exactly one new one of the same type appeared.
- **Renaming a table or column physically is explicit:** `k migrate new --rename <entity or field> "description"` writes the `renameTable` or `renameColumn` that makes the table or column follow the entity's or field's current name (`k migrate new --rename notes.Note.heading "rename the heading column"` renames `note.title` to `note.heading`), marked `@Breaking("single instance")`, and updates the lock. It is never inferred from a diff, so a rename never becomes a silent drop and create.

### Rolling deploys: expand, then contract

`dist/app serve` applies pending migrations at startup, and several replicas may run ([38-deployment.md](38-deployment.md)), so during a rolling deploy the previous release runs against the new schema. Migrations therefore follow expand/contract:

- **Removing a field retires its column.** The generated migration calls `table.retireColumn(name:)`, which makes the column nullable if it is `NOT NULL` without a default (so the next release's inserts can leave it out), and `schema.lock` marks the column `retired`; the app stops reading and writing it. The previous release still can. A table is retired the same way with `schema.retireTable(name:)`, written by hand for a removed entity.
- **Retirement is a builder call, not only a lock entry,** so replaying the migrations produces the `retired` marks the lock records, and the replay knows, for each later drop, whether an earlier migration retired what it drops.
- **A column or table can be dropped only once it is retired by an earlier migration.** `k migrate new --drop-retired "description"` writes that migration (marked `@Breaking("drops what earlier migrations retired")`) and removes the dropped tables and columns from the lock; it refuses to drop a retired table that a live column still references. `k check` fails on a `dropColumn` or `dropTable` of anything still live, with a message explaining the two-release sequence: ship the retirement, then the drop.
- **A physical rename becomes add, backfill, retire, drop** across two releases. A direct `renameColumn` or `renameTable` is allowed only with the opt-out below.
- **Single-instance apps opt out** with `@Breaking("single instance")` on the migration, which allows dropping or renaming a live column in one step. SQLite apps on one node usually do.

### Data migrations

- **Data migrations run in every environment, tests included.** They are for reference data every environment needs (countries, plans, permissions). `k test` applies every migration to each run's fresh database, so every test sees those rows.
- **Development-only seed data does not belong in a migration,** since it would ship to production. A separate mechanism is an open question below.

### The `schema.lock` file

`schema.lock` is TOML, generated and committed. It records the schema the migrations produce, so `k check` can compare it with the entities without a database:

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

- Each column records the field it belongs to and its locked name. A retired column has no field. A retired table has `retired = true` after its `entity`.
- `k check` fails when **the entities differ from the lock** (`K8004`, "run `k migrate new`", or `k migrate rename` for a rename), when **replaying `migrations/` doesn't produce the lock** (`K8005`: a hand-edited lock or migration), when file numbers don't match the `migrations` list or a listed file is missing (`K8006`), when a migration doesn't replay (`K6004`), and when a destructive migration lacks its `@Breaking` (`K6005`, `K6006`). The lock is compared without a database. These checks apply to apps (projects with an `App.k`), which own a database; they run once the program has no other errors.
- **Who writes it:** `k migrate new` and `k migrate rename` write it with their change. `k build` records what the migrations produce: the tables, columns, cases, options and retirements of the replay, in the order of the `migrations` list (migrations the list doesn't name yet are appended), with each table's entity and each column's field carried over from the lock (through the physical renames the migrations made) or, for something new, taken from the entity and field stored there. It writes the file only when that differs from what the lock records. `k check` never writes it, so CI catches a lock nobody rebuilt.
- Tables managed by the stdlib (`k_session`, `k_job`, `k_rate_limit`) are not recorded. The stdlib migrates them.

## Why

- Strings in migrations and checked replay together give both stability and safety: history never changes meaning, and mistakes are still caught at compile time.
- Locked table and column names follow "lockfiles for anything outside systems depend on" ([01-principles.md](01-principles.md)). The database is such a system, and renaming code must never lose data.
- A builder instead of raw SQL keeps migrations portable between SQLite and Postgres. `execute` exists for the rare exception, and requiring both dialects keeps that portability visible.
- Built-in writes keep repositories focused on queries, the one thing that differs between them, as Spring Data does with `CrudRepository`. Not writing relations implicitly avoids ORMs' cascading-save surprises.
- Typed unique violations and `@Version` turn the two common write races into values the caller must handle.

## Open questions

- **Development-only seed data,** for example a `seeds/` directory that only `k dev` and end-to-end scripts load.
- Online migrations for large tables (backfilling a column in batches) need a way to run data steps outside the schema transaction.
