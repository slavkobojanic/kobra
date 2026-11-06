# Fuzz regressions: semantic analysis

Minimized inputs found by the `check*` fuzz targets (`fuzz/fuzz_targets/check*.rs`), pinned before the
checker is fixed ([testing.md §6.4](../../../../../docs/compiler/testing.md#64-when-a-target-crashes)). A
case with a `.pending` file is an open finding: it fails once the checker is fixed, and then the
`.pending` file is deleted.

| Case | Target | Symptom |
|---|---|---|
| `arguments-of-unresolved-callee-are-checked.k` | `check_typed` (fix property) | fixed in round 2: the arguments of `Missing.make(…)` were not checked when `Missing` didn't resolve, so an independent unresolved name was hidden until the import fix for `Missing` was applied, which then "added" errors. The `tgen` workaround (one missing import per file) is gone |
| `rpc-fix-import-below-declarations.k` | `check` | fixed in round 2: with the imports below the declarations (K0111), the K5501 fix's import edit came after its `throws` edit, so the fix's edits were out of source order |
