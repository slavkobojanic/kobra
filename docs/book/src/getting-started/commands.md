# Everyday commands

Four commands cover almost all day-to-day work: `k dev` while you build, `k test` while you test,
`k check` before you push, and `k build` when you ship. This chapter explains what each one does. The
[CLI reference](../reference/cli.md) lists every command.

## `k dev`

```bash
k dev
```

Builds the app and serves it, by default on `http://localhost:8080`. Leave it running: on every save
it rebuilds, and when the build has errors it says how many and keeps serving the last good build;
`k check` shows them.

- `k dev` builds the **`.development` environment**. Any `use … in .development` rules in `App.k`
  apply, so you can use a fake payment provider locally and the real one in production.
- **It needs no setup.** It loads configuration from the process environment and, if it exists, from a
  `.env` file at the project root; what they leave unset comes from the `[dev]` section of `k.toml`
  (development values and generated development secrets, see
  [Project files](../reference/project-files.md#development-settings)) and from development defaults.
  When several set a variable, the process environment wins, then `.env`.
- It only asks for configuration that the development dependency graph reaches, so a variable needed
  only by a production payment provider is never required locally. See
  [Configuration and secrets](../apps/config.md).
- If the app has a database, `k dev` uses the `DATABASE_URL` from your environment or `.env`, and
  otherwise a SQLite file, `.k/<package>.db`. It applies pending migrations at startup, runs `[dev] seed`
  once when it creates the database, and runs the background-job worker.
- If the app sends email and no `SMTP_HOST` is set, `k dev` catches it: each message is printed with its
  links and kept in `.k/mail/`.

- **On a change** `k dev` rebuilds and restarts the server on the same port. A build with errors is
  reported and the last good build keeps serving. An entity without its migration counts as an error
  here, as it does in `k check`, so the app is never served without its tables. A restart drops
  in-memory state; sessions and data are in the development database and survive it. Open streams end at once, and the page subscribes
  again.
- **At a workspace root** `k dev` builds and serves every app member, each on its own port (`8080`,
  `8081`, … in dependency order), wires each `remote` rule to the sibling that serves it, and prefixes
  each app's output with its name. Ctrl-C stops them all. See
  [Workspaces and services](../apps/workspaces.md).
- **The process environment wins over `.env`,** and `k dev` says so at every start: ``note: the process
  environment shadows these `.env` keys: DATABASE_URL``. Only keys set in both, to different values, are
  named, and never a value.
- `--port` changes the port. `k run` is the same without the rebuilding, at a workspace root too.

> **Planned** (hmr). State-preserving hot module replacement isn't in the first compiler releases. Until it
> lands, refresh the browser after a rebuild.

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
k check --no-tests
```

`k check` is the gate. It fails, with exit status `1`, on any problem:

1. **Type errors** and every other compile-time check: layering, the server/client boundary,
   exhaustive matches, class and page references, style values.
2. **Documentation.** References in `///` comments must resolve; one that doesn't is a warning.
3. **Lockfiles.** A breaking change to a protocol, a URL, the database schema or a stored format fails
   unless it is deliberate and marked with `@Breaking("reason")`.
4. **Tests.** Once the program has no errors, `k check` runs every test, as `k test` does. Each failed
   test is a `K7510` error, rendered like `k test`'s, and the summary counts the tests that ran:
   `checked 24 files, ran 53 tests: no problems`. `--no-tests` skips them, for a quick type check.

The lockfiles are `rpc.lock` (the protocols an app serves), `routes.lock` (URLs and redirects),
`schema.lock` (the database schema and the migrations) and `storage.lock` (stored keys and job payloads);
see [Project files](../reference/project-files.md#lockfiles). `k check` only reads them; `k build` writes
`rpc.lock`, `routes.lock` and `schema.lock`. The `routes.lock` and `schema.lock` comparisons run once the
program has no other errors, and the language server doesn't show them.

`k check` takes files or directories (the project by default). Run at a workspace root, it checks every
member, each with the libraries it depends on. It has no watch mode; it is the one-shot gate.

Only `k check` runs the tests. The language server never does, so the editor's diagnostics stay
instant however large the test suite grows.

> **Planned** (k-fmt, doc-blocks). The design adds two more steps to `k check`: the formatting check and type-checking the
> `k` code blocks inside `///` comments. Neither is part of `k check` yet.

Run it in CI. With `--format json`, diagnostics are machine-readable, including their quick fixes,
which is how agents apply fixes directly.

`k fmt` (**Planned** k-fmt) rewrites code into the canonical form, including removing redundant spellings that have a
machine-applicable fix, such as `public` on a member.

> **Planned** (k-fmt). The formatter, `k fmt`, isn't in the first compiler releases. Until it ships, the
> formatting step of `k check` isn't enforced, but redundant spellings are still reported as warnings
> with their fixes.

## `k fix`

```bash
k fix
k fix --dry-run
k fix --code K1401
```

Many diagnostics carry a machine-applicable quick fix: a `let` that should be `var`, a missing `await`, a
call to a [deprecated](../language/modules.md#deprecating-a-declaration) function whose replacement takes
the same arguments. `k fix` applies every one of them, checks again, and repeats until nothing is left to
fix, so a fix that makes another one possible is applied too. It then prints the problems that remain, as
`k check` does, and exits with `1` when any remain.

- **`--dry-run`** prints the changes as a unified diff instead of writing them.
- **`--code K1401`** applies only that code's fixes; repeat it for several codes.
- It takes the same paths as `k check`, and in a workspace a fix in a library is made in the library's
  own file. It never runs the tests, and never touches the standard library.

Fixes that need a human decision (a "did you mean" rename) are never applied; the editor offers them as
quick fixes instead.

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
  environment, `.env` or `[dev.env]`, else `.k/<package>.db`) has applied and the ones still pending; **`k migrate apply`** applies the
  pending ones. Production applies migrations itself at startup.

See [Persistence](../apps/persistence.md#migrations).

## Exit status

Every command exits with `0` on success, `1` when it found problems (a type error, a failing test), and
`2` on a usage or internal error. The one exception is invalid configuration: `k run`, `k dev` and
`dist/app` exit with `78` (`EX_CONFIG`) when a config value is missing or malformed (see
[Configuration and secrets](../apps/config.md#validated-at-startup-all-at-once)). Scripts and CI can rely
on these.
