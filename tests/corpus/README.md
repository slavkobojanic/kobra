# Golden corpus

Each directory here is a suite. Every `.k` file below a suite directory is one test case (in `sema/`, a
directory with a `k.toml` is one project case instead); its expectations are sibling files with the same
name and a suite-specific extension.

| Suite | Expectations |
|---|---|
| `lexer/` | `name.tokens` (required), `name.diag` (optional; absent means no diagnostics) |
| `parser/` | `name.ast` (required), `name.diag` (optional; absent means no diagnostics) |
| `sema/` | `name.diag` and `name.pending` (both optional); a directory with a `k.toml` is one project case with `expected.diag` and `expected.pending` inside it; caret annotations in the sources |

The formats, naming conventions and commands are documented in
[docs/compiler/testing.md](../../docs/compiler/testing.md).
