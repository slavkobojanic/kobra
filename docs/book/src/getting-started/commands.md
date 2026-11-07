# Everyday commands

Four commands cover almost all day-to-day work: `k dev` while you build, `k test` while you test,
`k check` before you push, and `k build` when you ship. This chapter explains what each one does. The
[CLI reference](../reference/cli.md) lists every command.

## `k dev`

```bash
k dev
```

Builds the app and serves it, by default on `http://localhost:8080`. Leave it running: on every save
it rebuilds what changed and reports compile errors in the terminal.

- `k dev` builds the **`.development` environment**. Any `use … in .development` rules in `App.k`
  apply, so you can use a fake payment provider locally and the real one in production.
- It loads configuration from the process environment and, if it exists, from a `.env` file at the
  project root. When both set a variable, the process environment wins.
- It only asks for configuration that the development dependency graph reaches, so a variable needed
  only by a production payment provider is never required locally. See
  [Configuration and secrets](../apps/config.md).
- If the app has a database, `k dev` uses the `DATABASE_URL` from your environment, such as a SQLite
  file in `.k/`, applies pending migrations at startup, and runs the background-job worker.

- **On a change** `k dev` rebuilds and restarts the server on the same port. A build with errors is
  reported and the last good build keeps serving. A restart drops in-memory state such as sessions.
- **At a workspace root** `k dev` builds and serves every app member, each on its own port (`8080`,
  `8081`, … in dependency order), and wires each `remote` rule to the sibling that serves it. See
  [Workspaces and services](../apps/workspaces.md).
- `--port` changes the port. `k run` is the same without the rebuilding.

> **Planned.** State-preserving hot module replacement isn't in the first compiler releases. Until it
> lands, refresh the browser after a rebuild.

> **Planned.** The design has `k dev` print which `.env` keys the process environment shadowed. The
> first compiler releases apply the precedence silently.

## `k test`

```bash
k test
k test --filter "Cart"
k test --watch
```

Runs every `test` declaration: unit tests, component tests and integration tests. A test is one
browser talking to one server, in one process: views render in the browser half, protocol calls reach
the server half through the real wire format, and the server's database is a fresh in-memory SQLite
database with every migration applied, so each test sees only what it wrote itself.

The report has one line per test, then each failure with its source, like a compile error. A failed
`expect` shows the value of each part of the expression:

```text
running 3 tests
PASS  Cart › starts empty
PASS  Cart › adds a product
FAIL  Cart › totals the items

failures:

error[K7510]: expectation failed: cart.total == 48.00.usd
  --> src/store/Cart.test.k:21:16
   |
21 |         expect cart.total == 48.00.usd
   |                ^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: `cart.total` was $24.00
   = note: `48.00.usd` was $48.00
   = note: in test `Cart › totals the items`

test result: FAILED. 2 passed; 1 failed; 0 filtered out
```

- **`--filter text`** runs only the tests whose full name (`Group › description`) contains the text.
  Repeat it to run several sets. A filter that matches nothing is an error, so a typo doesn't pass.
- **`--watch`** runs the tests, then keeps running: on every save it re-runs the test files that
  changed or that use, directly or through other files, a file that changed.
- **Exit status:** `0` when every test passed, `1` when one failed or the project has errors (they are
  printed and nothing runs), `2` when the directory isn't a project.

Tests never read your environment or your `.env` file, and every test starts from a fresh container,
so they behave the same on every machine. See [Testing](../apps/testing.md).

## `k check`

```bash
k check
k check --format json
```

`k check` is the gate. It fails, with exit status `1`, on any problem:

1. **Type errors** and every other compile-time check: layering, the server/client boundary,
   exhaustive matches, class and page references, style values.
2. **Documentation.** References in `///` comments must resolve; one that doesn't is a warning.
3. **Lockfiles.** A breaking change to a protocol, a URL, the database schema or a stored format fails
   unless it is deliberate and marked with `@Breaking("reason")`.

> **Planned.** In the first compiler releases, `k check` compares two lockfiles: `rpc.lock` (the rpcs an
> app serves) and the job payloads in `storage.lock`. Comparing `protocol.lock`, `routes.lock`,
> `schema.lock` and the `storage.lock` keys comes later; the build already checks that the migrations
> create what the entities store (see [Persistence](../apps/persistence.md#schemalock)).

`k check` takes files or directories (the project by default). Run at a workspace root, it checks every
member, each with the libraries it depends on. It has no watch mode; it is the one-shot gate.

> **Planned.** The design adds three more steps to `k check`: the formatting check, running every test,
> and type-checking the `k` code blocks inside `///` comments. None of them is part of `k check` yet, so
> CI runs `k test` as well.

Run it in CI. With `--format json`, diagnostics are machine-readable, including their quick fixes,
which is how agents apply fixes directly.

`k fmt` rewrites code into the canonical form, including removing redundant spellings that have a
machine-applicable fix, such as `public` on a member.

> **Planned.** The formatter, `k fmt`, isn't in the first compiler releases. Until it ships, the
> formatting step of `k check` isn't enforced, but redundant spellings are still reported as warnings
> with their fixes.

## `k build`

```bash
k build
```

Builds the **`.production` environment** into `dist/`:

- **`dist/app`** is a single, self-contained server executable: the server bundle, the JavaScript
  runtime, every hashed client asset and every migration, in one file.
- **`dist/public/`** holds the same client assets, for teams that serve them from a CDN.

An app that renders entirely in the browser and has no server-only code produces only `dist/public/`,
which any static host can serve. See [Deployment](../apps/deployment.md).

## `k routes`

```bash
k routes
```

Prints the app's route table: one row per URL and method, for every page, every handler of an endpoint
page and every redirect recorded in `routes.lock` for a renamed page, with the parameters (query
parameters with their defaults) and the guards.

```text
METHOD  URL              PAGE      PARAMS                    GUARDS
GET     /                Notes                               Authenticated
GET     /edit-note/{id}  EditNote  id: Uuid                  Authenticated
GET     /sign-in         SignIn    ?returnTo: Page = .notes
```

## `k migrate`

```bash
k migrate new "add note pinning"
k migrate new --empty "backfill display names"
k migrate status
k migrate apply
```

- **`k migrate new "description"`** writes the next migration file from the difference between your
  entities and `schema.lock`, and updates the lock. It writes new tables, new columns that have a default
  or an optional type, new indexes and the retirement of a removed field. A change it can't make safely,
  such as a new required column without a default or a changed type, is refused: nothing is written and
  each refused change is explained. Review the file and commit both.
- **`--empty`** writes an empty migration, for data changes you write by hand.
- **`k migrate status`** lists the migrations the development database (`DATABASE_URL`, from the
  environment or `.env`) has applied and the ones still pending; **`k migrate apply`** applies the
  pending ones. Production applies migrations itself at startup.

See [Persistence](../apps/persistence.md#migrations).

## Exit status

Every command exits with `0` on success, `1` when it found problems (a type error, a failing test), and
`2` on a usage or internal error. The one exception is invalid configuration: `k run`, `k dev` and
`dist/app` exit with `78` (`EX_CONFIG`) when a config value is missing or malformed (see
[Configuration and secrets](../apps/config.md#validated-at-startup-all-at-once)). Scripts and CI can rely
on these.
