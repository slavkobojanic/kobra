# Golden corpus

Each directory here is a suite. Every `.k` file below a suite directory is one test case; its
expectations are sibling files with the same name and a suite-specific extension.

| Suite | Expectations |
|---|---|
| `lexer/` | `name.tokens` (required), `name.diag` (optional; absent means no diagnostics) |
| `parser/` | `name.ast` (required), `name.diag` (optional; absent means no diagnostics) |

The formats, naming conventions and commands are documented in
[docs/compiler/testing.md](../../docs/compiler/testing.md).
