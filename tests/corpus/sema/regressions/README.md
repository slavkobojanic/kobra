# Regressions: semantic analysis

Checker findings pinned as corpus cases ([testing.md §7](../../../../docs/compiler/testing.md#7-regression-tests)).
Fuzz findings are in [`fuzz/`](fuzz/README.md). A case with a `.pending` file is an open finding: it
fails once the checker is fixed, and then the `.pending` file is deleted together with the workaround
that cites it (`KNOWN_FINDINGS` in `crates/k_sema/tests/support/tgen.rs`).

Every case here is fixed (round 2); none is pending. The table lists the property-found ones; the other
cases' comments name their finding and the rule they pin.

| Case | Found by | Symptom (fixed) |
|---|---|---|
| `negated-if-with-optional-expected-type.k` | `well_typed_projects_have_no_errors` | `-(if c { 1 } else { 2 })` where `Int?` is expected was rejected with K2013: the optional expected type reached the operand of unary `-` |
| `missing-await-fix-in-binary-operand.k`, `missing-try-fix-in-binary-operand.k` | `machine_fixes_in_the_corpus_add_no_errors` | the K2204/K2201 fix inserted `await`/`try` inside a binary operand (K0114); it now goes before the covering expression |
