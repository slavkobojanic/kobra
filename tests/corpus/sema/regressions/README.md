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

Found by the stage 3 test writers (fixed in stage 4; the `run` cases
`run/regressions/tuple-literal-match-runs.k` and `list-contains-where-runs.k` pin the behaviour):

| Case | Symptom (fixed) |
|---|---|
| `tuple-literal-patterns-are-reachable.k`, `tuple-literal-patterns-unreachable.k` | a tuple pattern counted as a wildcard, so `case (0, true)` made every later arm K2302; a tuple is now its type's single constructor with the elements as fields (`pattern.rs`) |
| `list-contains-where-overload.k` | `xs.contains(where: { … })` resolved to `contains(_:)` (K2004, then K2002): overload selection ignored the labels of lambda arguments (`select_overload`) |
