---
name: kobra-tooling
description: The Kobra toolchain and workflow - the `k` command (k new, check, fix, fmt, test, dev, run, build, routes, migrate, upgrade, toolchain, keys, lsp, mcp), writing and running Kobra tests (test groups, expect, fakes with use … for …, render and accessibility queries, TestClock, TestClient), reading and fixing compiler diagnostics (K-codes), lockfiles, formatting and editor setup. Use when running, testing, debugging, formatting or verifying Kobra code, or when a .k file fails to compile.
---

# Kobra tooling

One binary, `k`, holds the compiler, test runner, formatter, development server, migration tool,
language server (`k lsp`) and MCP server (`k mcp`). The commands below come from its clap definitions
(`crates/k_cli/src/cli.rs`). `k <command> --help` is authoritative.

## The loop

1. Write the code, then run **`check_snippet`** (kobra MCP server) on each file you changed. Pass
   `file_name` and any sibling files in `files`. It runs the same analysis as `k check`, without tests
   or lockfiles.
2. Run **`k fmt`** on the changed files (or the whole project).
3. Run **`k check`** in the project. It runs syntax, types, layering, styles, lockfiles and then
   **every test**, and fails on any error *or warning*. `k check --no-tests` is the quick type check.
4. For an unfamiliar code, call `explain_diagnostic` with it (MCP), or `search_docs` the code. With
   neither available, read `docs/book/src/reference/diagnostics.md`. Run `k fix` to apply every
   machine-applicable fix, or `k fix --code K2012` for one code.
5. Run `k test --filter "Group"` while you iterate on tests.

Agents should prefer `k check --format json` (JSON Lines, fixes included). Exit status is `0` on
success, `1` when problems were found, `2` on a usage error or when the directory isn't a project,
and `78` for invalid configuration (`k dev`, `k run`, `dist/app`).

## Commands

| Command | What it does |
|---|---|
| `k new <name>` | creates an app in `<name>/` (lower case, digits, hyphens) that already passes `k check` |
| `k check [paths…] [--format human\|json] [--no-tests]` | the gate. It runs every check plus the tests, and reads lockfiles but never writes them |
| `k fix [paths…] [--code K…] [--dry-run]` | applies machine-applicable fixes until none remain, then reports the rest |
| `k fmt [paths… \| -] [--check] [--format json]` | canonical formatting, with no options. `--check` prints diffs and exits 1, and `-` reads stdin |
| `k test [project] [-f/--filter text]… [--watch]` | runs tests. A filter matching nothing is an error |
| `k dev [project] [--port 8080] [--release] [--inspect [port]]` | the dev server. It rebuilds and restarts on change, with no HMR yet |
| `k run [project] [--port …]` | like `dev`, without rebuilding |
| `k build [project] [--out dir]` | production build into `dist/` (`dist/app`, `dist/public/`). It writes `routes.lock`, `rpc.lock` and `schema.lock` |
| `k routes [project]` | the route table: pages, endpoints, redirects and guards |
| `k migrate new "<description>" [--empty \| --drop-retired \| --rename NAME…]` | writes the next migration from the difference between the entities and `schema.lock` |
| `k migrate rename <old> <new>` | records an entity or field rename in `schema.lock` (`notes.Note.title heading`) |
| `k migrate apply`, `k migrate status` | apply migrations to, or inspect, the development database |
| `k upgrade [--to VERSION] [--allow-dirty]` | moves a project to a toolchain, applying migration fixes |
| `k toolchain install [VERSION]`, `list`, `remove V`, `default V` | manage installed toolchains (`stable`, `nightly`, `0.3`, …) |
| `k self update` | updates the `k` on the PATH |
| `k hash-password` | reads a password on stdin and prints its Argon2id PHC string |
| `k keys new ed25519\|x25519\|p256-ecdsa\|aes-gcm` | prints a new key as a JWK |
| `k lsp` | the language server over stdio |
| `k mcp [--root DIR]` | the MCP server over stdio: docs, examples and the checker |

Details, including dev configuration, toolchain resolution and workspaces, are in
[references/cli.md](references/cli.md).

## Tests in one glance

```k
test GuestbookService {
    @Inject let guestbook: GuestbookService

    test "a signed entry is listed" {
        try await guestbook.sign(request: SignRequest(name: "Ana", message: "Hello!"))

        let entries = try await guestbook.entries()

        expect entries.count == 1
        expect entries[0].name == "Ana"
    }
}
```

- Test files sit next to the code as `Name.test.k`, in the same module.
- `expect <Bool expression>` is the only assertion, with no matchers. Write `expect not x`, never
  `== false`.
- With no fakes, a test runs the **real stack in-process**: real services, rpcs through the real wire
  format, a fresh in-memory SQLite database with every migration, a fresh signed-out session, and
  `TestClock`. Platform I/O always has a hermetic default.

The full guide (fakes, component tests, time, the database, HTTP-level tests) is in
[references/testing.md](references/testing.md). Reading diagnostics and the common codes are in
[references/diagnostics.md](references/diagnostics.md).

## Editors

`k lsp` gives diagnostics, quick fixes, go-to definition and implementation, rename, hover (an
effect's dependencies, a page's URL, token values), completion and formatting. Ready-made setups are
in `editors/vscode`, `editors/nvim` and `editors/vim`. This Claude Code plugin lives in
`editors/claude`. Its `.mcp.json` starts `k mcp`, which needs a `k` on the PATH that includes the
`mcp` command.
