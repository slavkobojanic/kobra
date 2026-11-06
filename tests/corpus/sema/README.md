# `sema` suite

Whole-project cases for semantic analysis: every diagnostic `k check` reports, plus caret annotations
for inferred types (`type:`), resolved names (`resolves:`) and server/client sides (`side:`).

- `name.k` is a single-file case at `src/corpus/name.k` (or the path on a `// path: …` first line).
- A directory with a `k.toml` is a project case; `expected.diag` and `expected.pending` inside it are
  its expectations, and `.diag` headers name their files.
- `*.pending` marks a test-first case for a check that doesn't exist yet: it must fail until the check
  lands, and then its `.pending` file is deleted.

One directory per area of [sema.md](../../../docs/compiler/sema.md); every code's span convention is in
[diagnostics.md](../../../docs/compiler/diagnostics.md#semantic-codes). Formats:
[testing.md §3.8](../../../docs/compiler/testing.md#38-the-sema-suite).
