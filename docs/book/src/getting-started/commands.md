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
  project root.
- If the app has a database, `k dev` uses the `DATABASE_URL` from your environment, such as a SQLite
  file in `.k/`, applies pending migrations at startup, and runs the background-job worker.

> **Planned.** State-preserving hot module replacement is deferred. Until it lands, a rebuild reloads
> the page.

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
4. **Lockfiles.** A breaking change to a protocol, a URL or the database schema fails unless it is
   deliberate and marked with `@Breaking("reason")`.

Run it in CI. With `--format json`, diagnostics are machine-readable, including their quick fixes,
which is how agents apply fixes directly.

> **Planned.** The formatter, `k fmt`, is deferred. Until it ships, the formatting step of `k check`
> isn't enforced.

## `k build`

```bash
k build
```

Builds the **`.production` environment** into `dist/`:

- **`dist/app`** is a single, self-contained server executable: the server bundle, the JavaScript
  runtime, every hashed client asset and every migration, in one file.
- **`dist/public/`** holds the same client assets, for teams that serve them from a CDN.

An app that renders entirely in the browser and has no server-only code produces only `dist/public/`,
which any static host can serve. See [Configuration and deployment](../apps/deployment.md).

## Exit status

Every command exits with `0` on success, `1` when it found problems (a type error, a failing test), and
`2` on a usage or internal error. Scripts and CI can rely on this.
