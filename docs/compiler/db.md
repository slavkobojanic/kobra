# Persistence

This document specifies persistence end to end: `k_db` (the schema model, the query IR, SQL for SQLite and
Postgres, the drivers, the `Schema` builder, migrations, `schema.lock`, the build's database manifest), how
`k_codegen` lowers repositories to it, and how `k_server` runs it. The language rules are
[19-persistence.md](../design/19-persistence.md); the overview is
[architecture.md §10](architecture.md#10-persistence).

Contents:

1. [Status and scope](#1-status-and-scope)
2. [Public API](#2-public-api)
3. [Schema model and naming](#3-schema-model-and-naming)
4. [The query IR and lowering](#4-the-query-ir-and-lowering)
5. [SQL generation](#5-sql-generation)
6. [Drivers, the manifest and ops](#6-drivers-the-manifest-and-ops)
7. [Writes, `@Version` and unique violations](#7-writes-version-and-unique-violations)
8. [Relations](#8-relations)
9. [Transactions](#9-transactions)
10. [Migrations](#10-migrations)
11. [`schema.lock`](#11-schemalock)
12. [Testing](#12-testing)
13. [Performance budgets](#13-performance-budgets)

---

## 1. Status and scope

| Part | Where | Status |
|---|---|---|
| Query IR | `k_db::ir` | **implemented** |
| SQL rendering, both dialects | `k_db::sql` | **implemented**, golden-tested |
| Schema model and DDL | `k_db::schema` | **implemented**: tables, unique and plain indexes, foreign keys, enum checks, money columns |
| `Schema` builder replay | `k_db::builder` | **implemented**: `createTable`, `alterTable` (`addColumn`, `renameColumn`, `dropColumn`), `renameTable`, `dropTable`, `createIndex`, `insert`, `execute` |
| Database manifest | `k_db::manifest` | **implemented** |
| Drivers: SQLite (`rusqlite`, bundled), Postgres (`tokio-postgres`) | `k_db::driver` | **implemented**, one connection per `Database` |
| Migrations runner, `k_migrations`, Postgres advisory lock | `k_db::migrate` | **implemented** |
| `schema.lock` read/write | `k_db::lock` | **implemented** |
| Lowering repository queries and writes to the IR | `k_codegen::db` | **implemented** (§4, §7, §8) |
| Process database, ops, request-bound transactions, savepoints | `k_server::db`, `k_server::ops` | **implemented** (§6, §9) |
| Column codecs, `Database`, repository helpers | `k_runtime` `runtime/db.js` | **implemented** |
| Entity/migration consistency | `k_codegen::db` | **implemented** as a build error (§10); the `k check` diagnostic in `k_sema` is planned |
| Connection pool | `k_server::db` | **planned**: v0 leases its one connection (§9) |
| Commit hooks (jobs, broadcasts after commit) | runtime | **planned**: broadcasts publish immediately in v0 |
| `k migrate new`, enum case operations | `k_sema`/`k_db` | **planned** (§10) |

**Deferred:** Postgres TLS (v0 connects without TLS, for a database on a private network or the same
host), online migrations, seed data, connection pooling.

---

## 2. Public API

| Item | Purpose |
|---|---|
| `ir::{Statement, Select, Predicate, Operand, ArithOp, CmpOp, Like, Order, Count}` | The query IR (§4). |
| `sql::{render, Dialect, Sql, quote_identifier, like_pattern}` | SQL text for a statement (§5). |
| `schema::{Schema, Table, Column, ColumnType}`; `Column::{definitions, stored_names}`, `Table::{create_sql, index_sql}` | The schema and its DDL (§3). |
| `builder::{Replay, SchemaOp, AlterOp, ColumnDef, Literal, Rendered}` | Migrations replayed into both dialects (§10). |
| `manifest::{DbManifest, StatementSql, UniqueIndex}` | The build's statements, migrations and unique indexes (§6). |
| `decimal::{compare, COLLATION}` | SQLite's `k_decimal` collation (§3). |
| `driver::{Database, DatabaseOptions}`; `Database::{connect, dialect, execute, query, ping}` | The drivers (§6). |
| `value::{Value, Row}` | Parameters and results. |
| `error::DbError::{InvalidUrl, Connection, UniqueViolation, Statement, UnsupportedType, Migration}` | Only `UniqueViolation` is a domain outcome; the rest become panics. Messages never contain a password or a bound value. |
| `migrate::{apply, applied, Migration, MIGRATIONS_TABLE, ADVISORY_LOCK_KEY}` | §10. |
| `lock::{SchemaLock, LockedTable, LockedColumn, HEADER}` | §11. |
| `testing::{test_databases, TestDatabase, POSTGRES_URL_ENV}` | §12. |

`k_server::db::{ProcessDatabase, DatabaseSetup, WriteOutcome, Savepoint}` run the manifest's statements for
a server process (§6, §9).

---

## 3. Schema model and naming

- **Tables** are named after their entity in snake case (`OrderLine` → `order_line`), **columns** after
  their field (`isPinned` → `is_pinned`); both are recorded in `schema.lock` when first created and kept
  across renames ([§11](#11-schemalock)). Codegen uses the locked names (matching the lock's `entity` to
  the qualified entity name and each column's `field`), and snake case for what the lock doesn't record.
- **Identifiers are always quoted** (`"order"`, `"user"`), with `"` doubled inside. SQLite connections turn
  off its double-quoted-string misfeature (`SQLITE_DBCONFIG_DQS_DML`/`_DDL`), so an unknown `"name"` is an
  error on both databases, never a string literal (tested).
- **Indexes**: `@Unique` → `CREATE UNIQUE INDEX "<table>_<column>_key"`, which is also Postgres' name for a
  unique constraint, so a violation maps to the field by name; `@Index` and every foreign key →
  `"<table>_<column>_idx"`. A money column's index covers both stored columns.
- **Column types** (`ColumnType`):

  | Type | K | Postgres | SQLite |
  |---|---|---|---|
  | `uuid` | `Uuid` | `uuid` | `TEXT` |
  | `text` | `String`, `EmailAddress`, `Url` | `text` | `TEXT` |
  | `integer` | `Int`, `Duration` (milliseconds) | `bigint` | `INTEGER` |
  | `decimal` | `Decimal` | `numeric(38,10)` | `TEXT COLLATE k_decimal` (SQLite would round a `REAL`) |
  | `boolean` | `Bool` | `boolean` | `INTEGER` 0/1 |
  | `timestamp` | `Date` | `timestamptz` | `TEXT`, RFC 3339 UTC with milliseconds |
  | `money` | `Money` | `<name>_amount numeric(19,4)` + `<name>_currency char(3)` | `TEXT COLLATE k_decimal` + `TEXT` |
  | `json` | a `Codable` value | `jsonb` | `TEXT` |
  | `enumeration` | a payload-free enum | `text` + `CHECK (… IN (…))` | `TEXT` + the same `CHECK` |

- The app's `AuthUserId` type (a struct with one stored field, `AccountId { value: Uuid }`) is stored as
  its field's column. `Double` has no column type in v0: an entity with a `Double` field fails the build
  ("store a `Decimal`"); sema's K6002 decides which types are allowed at all.
- **The `k_decimal` collation.** SQLite compares text as text (`"10.0" < "9.6"`), so every SQLite
  connection registers `k_decimal` (`decimal::compare`: by value, `"1.50"` = `"1.5"`, total on all text)
  and the DDL declares decimal and money-amount columns with it. Comparisons, `ORDER BY` and indexes on
  them then follow the numbers, as `numeric` does on Postgres (tested on both).
- `T?` fields are nullable; every other column is `NOT NULL`. A to-one `@Relation` field is a
  `<field>_id` column `REFERENCES "<table>" ("id")`; a to-many relation has no column.
- The `k_` prefix is reserved: `k_migrations`, `k_session`, `k_job`, `k_rate_limit` are managed by the
  stdlib and not recorded in the lock.

---

## 4. The query IR and lowering

`k_sema` checks repository bodies (K6001 for calls SQL can't express); `k_codegen::db` lowers each
**awaited** chain on the repository's collection (`notes` in `repository NoteRepository for Note`) into one
`ir::Statement`. Anything it can't lower fails the build with the file, line and reason, never an
in-memory scan.

| K | IR |
|---|---|
| `notes` (the entity collection), awaited | `Select { table, columns: every stored column in field order }` |
| `.filter { n => P }` (repeatable; filters are `And`ed) | `filter: Some(lower(P))` |
| `n.field == x`, `!=`, `<`, `<=`, `>`, `>=` | `Compare { Column, op, Param }` (either side may be the column; flipped when it is on the right) |
| `n.a == n.b` | `Compare { Column, op, Column }` |
| `n.field == nil` / `!= nil` | `IsNull` / `IsNotNull` |
| `n.field == x` with `x: T?` | `field = x OR (field IS NULL AND x IS NULL)`: `nil` equals `nil`, as in K |
| `n.flag` (a `Bool` field on its own) | `Compare { Column, Eq, Param(true) }` |
| `n.price <= m` (money) | `amount <= p AND currency = p'` |
| `a and b`, `a or b`, `not a` | `And`, `Or` (flattened), `Not` |
| `ids.contains(n.id)` | `In { operand, list }`, one parameter holding the list |
| `n.title.contains(t)`, `startsWith`, `endsWith` (text fields) | `Like { column, kind, param }` |
| `n.relation.id == x` | `Compare` on the `<relation>_id` column |
| `.sortedBy { n => n.f }`, `…Descending`, `.thenBy…` | `order` (a new `sortedBy` replaces the order; money sorts by amount) |
| `.limit(n)`, `.offset(n)` | `Count::Const` for an integer literal, `Count::Param` otherwise |
| `first { … }` / `single { … }` | the predicate `And`ed, `LIMIT 1` / `LIMIT 2` |
| `.count()` | `Count(select)` |
| `.update { n => n.f = v; n.g += x }` | `Update { set, filter }`: `=` a parameter or another column, `+=`/`-=` on `Int` fields `Arith { Column, Add/Sub, Param }` |
| `.delete()` | `Delete { filter }` |
| `.including { n => n.rel }` | a second statement per relation ([§8](#8-relations)) |
| `notes.insert(x)`, `insert(all:)`, `update(x)`, `delete(x)` | the inherited writes ([§7](#7-writes-version-and-unique-violations)) |

- **Every value is a parameter.** Captured K values and constants (`n.isPinned == true`, a case `.live`)
  become `Param(n)`, numbered from 1 in order of use; only integer literals of `limit`/`offset` are inlined.
  A value may not mention the row (`n.a == n.b + 1` is a build error: "only a field of the row can be
  compared").
- **Parameters are encoded by the column's codec** (§6): `Bool` as a boolean (SQLite 0/1), `Date` as RFC
  3339 UTC with milliseconds, `Uuid`, enum cases and `Decimal` as text, `Money` as two parameters, an
  `AuthUserId` as its value, a list as JSON array text, a `LIKE` text through `likePattern` (escaping `\`,
  `%`, `_` exactly like `sql::like_pattern`).
- **The generated code** is one call per awaited query, so a query is a single expression wherever it
  appears: `$s.dbQuery(this.$db, kind, statement, $s.dbParams([[codec, value], …]), decode, includes,
  "Note")` (`kind` 0 list, 1 `first`, 2 `single`), `$s.dbCount(…)`, `$s.dbWrite(…)`. Identical statements
  share one number.
- **Rows decode** into entities by `R$E$dbRow(r)`, which calls the entity's constructor with each field's
  codec at its column offset; a to-many relation starts as `unloadedMany`, a to-one as an `unloaded`
  stand-in that knows only its `id` (§8). The codec table `R$E$dbCols()` is built on first use, since it
  refers to enums and constructors declared later in the module.

---

## 5. SQL generation

`sql::render(statement, dialect) -> Sql { text, params }`. The output for every construct is pinned by
`crates/k_db/tests/sql.rs`:

| IR | SQLite | Postgres |
|---|---|---|
| parameter `n` | `?n` | `$n` |
| `Compare` | `"a" = ?1` (`<>` for `!=`) | `"a" = $1` |
| `Arith` | `("stock" - ?1)` | `("stock" - $1)` |
| `IsNull` / `IsNotNull` | `"a" IS NULL` / `IS NOT NULL` | same |
| `And`/`Or` | joined with ` AND `/` OR `, parenthesized when nested; empty `And` is `1 = 1`, empty `Or` `1 = 0` | same |
| `Not` | `NOT (…)` | same |
| `In` | `"id" IN (SELECT value FROM json_each(?1))` with a JSON array parameter | `"id" = ANY($1)`, the JSON array bound as an array of the column's type |
| `Like` | `"title" LIKE ?2 ESCAPE '\'` | same |
| order | ` ORDER BY "a" DESC, "b"` | ` ORDER BY "a" DESC NULLS LAST, "b" NULLS FIRST` |
| limit/offset | ` LIMIT 50 OFFSET ?1`; offset alone ` LIMIT -1 OFFSET …` | offset alone ` LIMIT ALL OFFSET …` |
| `Count` | `SELECT COUNT(*) FROM "t" WHERE …` | same |
| `Insert` (`rows` × columns) | `INSERT INTO "t" ("a", "b") VALUES (?1, ?2), (?3, ?4)` | `$1…` |
| `Update` | `UPDATE "t" SET "a" = ?1 WHERE …` | same |
| `Delete` | `DELETE FROM "t" WHERE …` | same |

- **`nil` order**: `nil` sorts before every value ascending and after them descending on both databases.
  That is SQLite's default; Postgres (whose default is the opposite) gets `NULLS FIRST`/`NULLS LAST`.
- **String predicates** bind `like_pattern(kind, text)`: `\`, `%` and `_` are escaped with `\`, then `%`
  is added where the predicate needs it. **SQLite connections set `PRAGMA case_sensitive_like = ON`**, so
  `LIKE` is case-sensitive on both databases (tested).
- The SQL for every statement is rendered at build time for both dialects into the build's database
  manifest (§6); the server picks the dialect of `DATABASE_URL`.

---

## 6. Drivers, the manifest and ops

- `Database::connect(url, options)`: `sqlite::memory:`, `sqlite:<path>` (created if missing), or
  `postgres://…`/`postgresql://…`. `DatabaseOptions.password` (`DATABASE_PASSWORD`, possibly from a secret
  file) overrides a password in the URL. Errors never echo the URL; `Debug` prints only the dialect.
- **SQLite** runs on tokio's blocking pool behind a mutex, with `foreign_keys = ON`,
  `case_sensitive_like = ON`, double-quoted strings off, the `k_decimal` collation, and a statement cache.
- **Postgres** uses `tokio-postgres`; statements without parameters go through the simple protocol (DDL,
  transaction control), and their count is the sum of the rows the statements affected.
- **Values**: `Null`, `Bool`, `Int` (i64; narrowed to `int2`/`int4` columns with an overflow error),
  `Double`, `Text`, `Bytes`. On Postgres, `Text` binds in binary into `uuid`, `numeric` (decimal text),
  `timestamptz`/`timestamp` (RFC 3339 text), `jsonb`/`json`, and any array type (a JSON array, for `In`);
  reading converts `bool`, `int2/4/8`, `float4/8`, `text`/`varchar`/`bpchar`/`name`, `bytea`, `uuid`,
  `numeric`, `timestamptz` (RFC 3339 UTC with milliseconds) and `jsonb`/`json` to the same values SQLite
  returns (`driver::pgcodec`); other types are `UnsupportedType`.
- **Decimals read back normalized** (no trailing fractional zeros) on both databases, since Postgres'
  `numeric(38,10)` pads every value to ten places: `10.0` reads as `10`.
- **The database manifest** (`manifest::DbManifest`) is part of the build output (`Output::database`) and
  of `dist/app`'s payload: `statements` (each `StatementSql { sqlite, postgres }`, its index its number),
  `migrations`, and `unique` (`UniqueIndex { table, columns, index, field }` per `@Unique` field).
- **The process database** (`k_server::db::ProcessDatabase`): `Server::start` opens `DATABASE_URL`
  (`DatabaseSetup::from_config`), applies the pending migrations and loads the statements of its dialect
  before any worker starts; a failure stops startup (`dist/app` exits 1). The database lives on a runtime
  of its own (the `k-db` thread), since workers have their own single-threaded runtimes and a Postgres
  connection must keep running between requests.
- **Ops** ([server.md §4](server.md#4-ops)): `op_k_db_query(statement, params, tx)` and
  `op_k_db_execute(…)` run statement `statement` (never SQL text from JavaScript) on transaction `tx` (0:
  none) and return rows as value lists in select order, or `{count}` / `{unique: field}`;
  `op_k_db_begin`, `op_k_db_finish(tx, commit)`, `op_k_db_savepoint(tx, depth, op)`, `op_k_db_ping(ms)`.
  Parameters are JSON scalars (an integral number is an `Int`); objects and arrays are refused. Every
  `DbError` but a mapped unique violation is an op error, which the runtime turns into a panic.
- **Column codecs** (`runtime/db.js` `dbc`): `uuid`, `text`, `email`, `int`, `bool`, `decimal`, `date`,
  `money` (two columns), `enumeration(cases, names)`, `json(codec)`, `optional(c)`, `wrap(make, field, c)`
  (one-field structs), `ref(idCodec, owner)` (to-one relations). A malformed column value panics instead of
  becoming a wrong value.

---

## 7. Writes, `@Version` and unique violations

- Every repository class gets `$insert($c, e)`, `$insertAll($c, es)`, `$update($c, e)` and `$delete($c, e)`.
  `repo.insert(x)` on a repository value, and `notes.insert(x)` on the collection inside it, lower to them.
- `insert(entity)`: one `INSERT`. `insert(all:)`: the build renders multi-row inserts of 64, 16, 4 and 1
  rows; `dbInsertAll` splits the entities greedily over them and runs more than one statement in a
  transaction (a savepoint inside an open one), so it is all or nothing. `update(entity)`: every column
  but the id and the relations, `WHERE "id" = ?`. `delete(entity)`: `WHERE "id" = ?`.
- **`@Version`**: `update(entity)` sets the column to `("revision" + 1)`, adds `AND "revision" = ?` with
  the copy's value, and returns `Bool` from the affected-row count (`false`: changed or gone since read).
- **Unique violations.** SQLite reports `SQLITE_CONSTRAINT_UNIQUE` (2067) or `…_PRIMARYKEY` (1555) with
  `UNIQUE constraint failed: <table>.<column>[, …]`; Postgres SQLSTATE `23505` with the constraint name.
  Both become `DbError::UniqueViolation { table, columns, constraint }`, which
  `DbManifest::unique_field` maps to the field by index name (Postgres) or table and stored columns
  (SQLite); `op_k_db_execute` answers `{unique: field}` and `dbWrite` throws the entity's
  `UniqueViolation` case for it: `{ $: i }`, `i` the field's position among the entity's `@Unique` fields.
  `catch .email` after a repository write tests that tag. A primary key or another index maps to nothing:
  a panic (a programmer error: ids are generated).

---

## 8. Relations

`.including { n => n.tags }` runs the main query, then **one query per relation**, never one per row
(`dbInclude`):

- to-many: `SELECT … FROM "tag" WHERE "note_id" IN (…ids of the main rows…)`, grouped by foreign key in
  memory and stored in each row, in the database's order; rows without children get `[]`;
- to-one: `SELECT … FROM "product" WHERE "id" IN (…distinct foreign keys…)`; a key without a row panics.

Reading an unloaded relation panics with the relation's name ("`Note.tags` was not loaded; add
`.including { … }`"); an unloaded to-one relation still knows its `id`. Copying an entity
(`var edited = current`) doesn't touch the stand-ins.

---

## 9. Transactions

- `database.transaction { … }` begins a transaction bound to the **request**: the request's `Database`
  (one per request container) passes its transaction id to every repository call inside. The block's value
  is returned; a throw or a panic rolls back and is rethrown.
- **Nesting** uses savepoints (`SAVEPOINT "k_sp_<n>"` / `RELEASE` / `ROLLBACK TO` then `RELEASE`): an
  inner throw rolls back to the savepoint and rethrows; the outer block may catch it and commit.
- **The lease.** v0 has one connection per process. A statement outside a transaction takes the
  connection's lease for the statement; `BEGIN` takes it until `COMMIT`/`ROLLBACK`, so other requests wait
  instead of running inside someone else's transaction (tested). Waiting is bounded (30 s): a transaction
  held that long makes the waiting statement panic rather than hang the server. `ping` doesn't wait.
- Isolation: `BEGIN ISOLATION LEVEL READ COMMITTED` on Postgres; `BEGIN IMMEDIATE` on SQLite (writers
  serialize up front instead of failing with `SQLITE_BUSY`).
- **Commit hooks** (jobs enqueued, broadcasts) fire when the outermost transaction commits and are
  discarded on rollback (design 19). *Planned*: v0 publishes broadcasts immediately.
- **Tests** (`k test`, planned): each test runs inside a transaction that is rolled back; the app's
  outermost transaction becomes a savepoint inside it.

---

## 10. Migrations

- **Files** `migrations/NNNN_description.k` hold one `migration` declaration whose `up(schema:)` calls the
  `Schema` builder. `k_codegen::db::migrations` reads the calls (strings, lists, maps and `ColumnValue`s
  only: a migration can't compute) into `builder::SchemaOp`s, in the order of `schema.lock`'s `migrations`
  list (file-name order without a lock), and `builder::Replay` applies them to its model of the schema,
  checking every name against what the earlier migrations created, and renders each migration's
  statements for both dialects (`migrate::Migration { name, sqlite, postgres }`).
- **Replay checks** (a build error naming the migration): an unknown or duplicate table or column, the
  `k_` prefix, a reference to a missing table, a non-null column added without a default, a primary key or
  unique column added to an existing table, a default or row value of the wrong type, a missing value for
  a required column in `insert`, dropping a table still referenced, an empty `execute`. The enum case
  operations (`addEnumCase`, …) rewrite a `CHECK`, which SQLite can only do by rebuilding the table: they
  are refused in v0 ("use `execute(postgres:sqlite:)`").
- **Money** columns are one logical column: `renameColumn`, `dropColumn` and `createIndex` act on both
  stored halves. A money column has no single default or row value in v0.
- **Entities against migrations**: after the replay, every entity's table must exist with every stored
  column of the right type, or the build fails ("`notes.Note.revision` is stored in column
  `note.revision`, which the migrations don't create; run `k migrate new`"). Skipped for a program without
  migrations. The `k check` diagnostic (and comparing with `schema.lock`) is `k_sema`'s, planned.
- **Applying** (`migrate::apply`): create `k_migrations ("name" TEXT PRIMARY KEY, "applied_at")` if
  missing; refuse if the database records a migration the build doesn't contain, or if the applied ones are
  not the build's first ones in order (a migration inserted before an applied one would otherwise run last);
  run each pending migration **in one transaction together with its `k_migrations` row**, so a failed
  migration leaves nothing behind; return the applied names. A failure is `DbError::Migration { name,
  message }`; `dist/app` reports it and exits 1 before opening its port.
- **Concurrency** on Postgres: `apply` holds `pg_advisory_lock(ADVISORY_LOCK_KEY)` while migrating, so
  replicas starting together apply each migration once (tested with four).
- **`k migrate new`** (planned): diff the entities against `schema.lock`, write the next file and update
  the lock; renames never inferred; dropping requires the column to be retired by an earlier migration
  ([19-persistence.md](../design/19-persistence.md#rolling-deploys-expand-then-contract)).

---

## 11. `schema.lock`

`lock::SchemaLock::to_toml` writes the canonical layout; `parse` reads any TOML of that shape and rejects
unknown keys:

```toml
# Generated by `k migrate`. Do not edit by hand.
migrations = ["0001_init", "0002_add_review"]

[tables.order]
entity = "store.Order"
columns = [
    { field = "id", name = "id", type = "uuid", primaryKey = true },
    { field = "status", name = "status", type = "enumeration", cases = ["paid", "cancelled"], default = "paid" },
    { name = "coupon_code", type = "text", nullable = true, retired = true },
]
```

Tables are sorted by name; column keys appear in the fixed order `field, name, type, primaryKey,
nullable, unique, indexed, references, cases, default, retired`, and false or empty keys are omitted. A
`default` written as a TOML boolean or number (`default = false`) reads as its text. `parse(to_toml(lock))
== lock` for every lock (tested).

---

## 12. Testing

- **SQL golden tests** (`crates/k_db/tests/sql.rs`): the exact text of every construct in both dialects.
- **Builder tests** (`crates/k_db/tests/builder.rs`): the SQL of every builder operation, every replay
  check, and rendered migrations applied to each test database.
- **Lock tests** (`lock.rs`, `lock_edges.rs`): the canonical layout, round trips, quoting, errors.
- **Driver tests** (`drivers.rs`, `errors.rs`, `queries.rs`, `values.rs`, `writes.rs`, …): every test runs
  against each database of `testing::test_databases()`: **SQLite always** (in memory), **Postgres when
  `K_TEST_POSTGRES_URL` is set** (each test in a fresh schema, dropped by `TestDatabase::finish`; a set but
  unreachable URL fails, because the run asked for Postgres).
- **Lowering tests** (`crates/k_codegen/tests/persistence.rs`): statements in the manifest and none in
  JavaScript, locked names, the inherited writes, shared statements, build errors.
- **The process database** (`k_server::db` unit tests): statements by number, typed unique violations,
  transactions and savepoints, the lease.
- **The runtime** (`crates/k_runtime/js/tests/db.test.js`): codecs, `dbQuery`, `dbInclude`,
  `dbInsertAll`, unique cases, transactions with fake ops.
- **The database corpus** (`tests/corpus/db`, [testing.md §3.11](testing.md)): apps served on a fresh
  SQLite database, and on Postgres when `K_TEST_POSTGRES_URL` is set, driven by `requests.http`.
- **The examples** (`crates/k_cli/tests/examples.rs`): notes and shop build, migrate and answer from the
  database.

  ```sh
  cargo test -p k_db                                       # SQLite
  cargo test -p k_server --test e2e -- db/                 # the db corpus on SQLite
  docker run -d --rm -p 55432:5432 -e POSTGRES_PASSWORD=pw -e POSTGRES_DB=k_test postgres:17-alpine
  K_TEST_POSTGRES_URL=postgres://postgres:pw@127.0.0.1:55432/k_test cargo test -p k_db
  K_TEST_POSTGRES_URL=postgres://postgres:pw@127.0.0.1:55432/k_test cargo test -p k_server --test e2e -- db/
  ```

---

## 13. Performance budgets

Measured in release on the development machine (`crates/k_db/tests/perf_budget.rs`,
`crates/k_server/tests/perf_budget.rs`, both `--ignored`):

| Area | Budget | Measured |
|---|---|---|
| `render` of a typical query | ≤ 2 µs (`k build` renders every statement twice) | 0.9 µs |
| Driver overhead per statement over the raw `rusqlite` call | ≤ 10 µs (one blocking-pool hop) | 5.9 µs |
| RPC with a primary-key query on SQLite, loopback, p50 | ≤ 250 µs (150 µs generated RPC + 100 µs database) | 76 µs |
| RPC with an indexed, sorted, limited 10-row query, p50 | ≤ 300 µs | 85 µs |
| Primary-key RPC throughput on SQLite (one connection) | ≥ 10,000 requests/s per process | 41,000 |
| `including` | one extra query per relation, independent of the row count | tested by statement counts |
