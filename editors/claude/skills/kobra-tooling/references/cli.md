# The `k` command in detail

Sources: `crates/k_cli/src/cli.rs` (the clap definitions), `book/getting-started/commands.md`,
`book/reference/cli.md` and `project-files.md`, and `design/13-tooling.md`.

## `k check`

- `k check [PATHS]...` (default `.`), `--format human|json`, `--color auto|always|never` and
  `--no-tests`.
- It checks types and every static rule, `///` doc links (K1901, a warning), lockfiles
  (`rpc.lock`, `routes.lock`, `schema.lock`, `storage.lock`, `k.lock`), and then runs the tests when
  the program has no errors. A failed test is a K7510 error.
- **Warnings fail it too.** Fix K2012, K1302, K3027 and the rest.
- The `routes.lock` and `schema.lock` comparisons run only once the program has no other errors, and
  the language server doesn't show them. Run `k check` itself.
- At a workspace root it checks every member.
- It doesn't check formatting yet (that's **Planned**), so run `k fmt --check` next to it in CI.

## `k fix`

`k fix [PATHS]... [--code CODE]... [--dry-run] [--format …]`. It applies machine-applicable fixes
repeatedly, then prints what remains (and exits 1 if anything does). It never runs tests. "Did you
mean" renames are never auto-applied.

## `k fmt`

`k fmt [PATHS]... [--check] [--format human|json]`. With no path it formats the whole project or
workspace, and `-` reads stdin and writes stdout. A file with syntax errors is left alone, and the
command exits 1. The layout is fixed: four-space indent, sorted imports, one blank line between view
nodes and between declarations, chained calls on their own lines when there are two or more, `init()`
after the fields, and redundant `public` on members removed. It keeps your line breaks and comments.

## `k test`

`k test [PROJECT] [-f|--filter TEXT]... [--watch] [--color …]`. Filters match the full name
`Group › description`. Tests never read the environment or `.env`.

## `k dev` and `k run`

`k dev [PROJECT] [--port 8080] [--release] [--inspect [PORT]]`

- It builds `.development` (the `use … in .development` rules apply), rebuilds and restarts on change,
  and keeps serving the last good build when a build fails. **There's no HMR**, so refresh the browser.
- Config comes from the process environment, then `.env`, then `[dev.env]` and `[dev.secrets]` in
  `k.toml`, then the defaults. The database defaults to `sqlite:.k/<package>.db`, migrated at startup,
  with `[dev] seed` run once. Mail is caught into `.k/mail/` when `SMTP_HOST` is unset. The job worker
  runs.
- At a workspace root it serves every app (8080, 8081, …), wired together.
- `--inspect` serves V8's inspector for the server's Kobra code (`chrome://inspect`), and `--release`
  builds the shipped client with the dev wiring. See `book/getting-started/debugging.md`.
- `k run` is the same without rebuilding.

## `k build`

`k build [PROJECT] [--out DIR]` builds `.production` into `dist/app` (one executable holding the
server, runtime, assets and migrations) and `dist/public/`. It records `routes.lock`, `rpc.lock` and
`schema.lock`, including redirects for renamed pages. At a workspace root it writes
`dist/<app>/`.

## `k migrate`

- `k migrate new "<description>" [--project DIR]` writes the safe changes only (new tables, new
  optional or defaulted columns, indexes, retirements) and refuses the rest with an explanation.
  - `--empty` writes an empty migration for hand-written changes and data.
  - `--drop-retired` drops columns and tables that earlier migrations retired.
  - `--rename NAME` (repeatable) physically renames a table or column to follow the current name, and
    is marked `@Breaking("single instance")`.
- `k migrate rename <old> <new>`: `k migrate rename notes.Note.title heading` (a field) or
  `k migrate rename notes.Note notes.Memo` (an entity) changes only `schema.lock`.
- `k migrate status` and `k migrate apply` work on the development `DATABASE_URL`.

## Other commands

- `k routes [PROJECT]` prints `METHOD URL PAGE PARAMS GUARDS`, including channels (`WS`).
- `k upgrade [PROJECT] [--to VERSION] [--allow-dirty]` raises `k = "…"` and `k.lock`, converts
  lockfiles, and applies deprecation fixes. Commit first.
- `k toolchain install [VERSION]` (default: the project's `k.lock`, else the newest allowed),
  `k toolchain list`, `k toolchain remove VERSION`, `k toolchain default VERSION`, and `k self update`.
  A project pins its toolchain with `k` in `k.toml` and `k.lock`, and a running `k` delegates to the
  matching installed toolchain. A missing one is K8201, which names the install command.
  `K_TOOLCHAIN=0.2.0-rc.1 k check` runs one command with a specific installed toolchain.
- `k hash-password` reads a password from stdin and prints its PHC string, for
  `SHOP_ADMIN_PASSWORD_HASH`-style configs.
- `k keys new ed25519|x25519|p256-ecdsa|aes-gcm` prints a private JWK for the environment, plus the
  public one.
- `k lsp` is the language server, over stdio.
- `k mcp [--root DIR]` is the MCP server (stdio). It serves the book, the design, the compiler docs,
  the examples and the current project, read-only, with the tools `list_docs`, `read_doc`,
  `search_docs`, `list_examples`, `read_example`, `explain_diagnostic` and `check_snippet`.
- **Planned, not built:** `k doc`, `k explain`, `k add` and `k remove` (git packages),
  `k proto export`, and `k new --library`.

## Lockfiles (generated, committed)

| File | Records | Written by |
|---|---|---|
| `routes.lock` | page and endpoint URLs, redirects | `k build` (add `[redirects]` or delete stale entries by hand) |
| `schema.lock` | tables, columns, migration order | `k migrate new`/`rename`, `k build` |
| `rpc.lock` | protocol wire names, field numbers | `k build` |
| `storage.lock` | local storage keys, collections, job payloads | the compiler |
| `k.lock` | the toolchain that last built the app | `k build`, `k upgrade` |

A breaking change to any of these needs `@Breaking("reason")` on the protocol, migration, job or
storage key.
