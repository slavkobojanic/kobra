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
  project root. When both set a variable, the process environment wins, and `k dev` prints which `.env`
  keys were shadowed.
- It only asks for configuration that the development dependency graph reaches, so a variable needed
  only by a production payment provider is never required locally. See
  [Configuration and secrets](../apps/config.md).
- If the app has a database, `k dev` uses the `DATABASE_URL` from your environment, such as a SQLite
  file in `.k/`, applies pending migrations at startup, and runs the background-job worker.

> **Planned.** State-preserving hot module replacement isn't in the first compiler releases. Until it
> lands, a rebuild reloads the page.

## `k test`

```bash
k test
k test --watch
```

Runs every `test` declaration: unit tests, component tests and integration tests that run the real
stack in-process against a fresh SQLite database. `--watch` keeps running and re-runs the affected tests
whenever you save a file.

Tests never read your environment or your `.env` file, and every test starts from a fresh container,
so they behave the same on every machine. See [Testing](../apps/testing.md).

## `k check`

```bash
k check
k check --format json
```

`k check` is the gate. It fails, with a non-zero exit status, on any problem:

1. **Type errors** and every other compile-time check: layering, the server/client boundary,
   exhaustive matches, class and page references, style values.
2. **Formatting.** Code that isn't in canonical form fails the check.
3. **Tests.** Every test runs.
4. **Documentation.** References in `///` comments must resolve, and `k` code blocks in them must type
   check. In a package, public API without a doc comment is a warning.
5. **Lockfiles.** A breaking change to a protocol, a URL, the database schema or a stored format fails
   unless it is deliberate and marked with `@Breaking("reason")`.

`k check` has no watch mode; it is the one-shot gate.

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

## `k migrate new`

```bash
k migrate new "add note pinning"
```

Writes the next migration file from the difference between your entities and `schema.lock`, and updates
the lock. Review the file and commit both. See [Persistence](../apps/persistence.md#migrations).

## Exit status

Every command exits with `0` on success, `1` when it found problems (a type error, a failing test), and
`2` on a usage or internal error. Scripts and CI can rely on this.
