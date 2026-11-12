# Golden corpus

Each directory here is a suite. Every `.k` file below a suite directory is one test case (in `sema/`, a
directory with a `k.toml` is one project case instead, and likewise in `codegen/` and `run/`; `db/` has project cases only); its expectations are sibling files with the same
name and a suite-specific extension.

| Suite | Expectations |
|---|---|
| `lexer/` | `name.tokens` (required), `name.diag` (optional; absent means no diagnostics) |
| `parser/` | `name.ast` (required), `name.diag` (optional; absent means no diagnostics) |
| `sema/` | `name.diag`, `name.fixed` and `name.pending` (all optional); a directory with a `k.toml` is one project case with `expected.diag`, `expected.fixed` and `expected.pending` inside it; caret annotations in the sources. `.fixed` is what `k fix` makes of the case (a project's lists each changed file after a `=== path` line), required exactly when the case has a machine-applicable fix |
| `codegen/` | at least one of `name.client.js`, `name.server.js`, `name.css`, `name.expect`; optional `name.pending`; projects use `expected.*` |
| `run/` | `name.stdout`, `name.result`, `name.pending` (all optional); executed in V8 by `cargo test -p k_server --test corpus_run` |
| `db/` | project directories only (`k.toml`, `App.k`, `src/`, `migrations/`, `.env`, `requests.http`); optional `expected.pending`; served on a fresh database by `cargo test -p k_server --test e2e -- db/` |

The formats, naming conventions and commands are documented in
[docs/compiler/testing.md](../../docs/compiler/testing.md).
