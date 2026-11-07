# Fuzz regressions: execution

Minimized programs found by the stage 3 differential properties (`crates/k_server/tests/differential.rs`)
and fuzz targets (`fuzz/fuzz_targets/codegen*.rs`, `rpc_json.rs`), pinned before the bug is fixed
([testing.md §6.4](../../../../../docs/compiler/testing.md#64-when-a-target-crashes)). A case with a
`.pending` file is an open finding: it fails once the bug is fixed, and then the `.pending` file is deleted
together with the generator workaround that cites it (`KNOWN_FINDINGS` in `tgen.rs`, `exprgen.rs`,
`codablegen.rs`, `viewgen.rs`).

Findings that need a release build or the client DOM, which the `run` suite doesn't use, are pinned in
`pinned_findings_still_reproduce` in `crates/k_server/tests/differential.rs` instead.

| Case | Found by | Symptom |
|---|---|---|
| `match-default-over-an-enum.k` | typed programs (`tgen` runnable mode) | a `match` over an enum with a `default` arm fails to load: two `default:` clauses in the generated `switch` |
| `map-key-proto-is-encoded.k` | codable round trip (`codablegen`) | `Json.encode` drops a `Map` entry whose key is `__proto__` (assigned to a plain `{}`) |
| `map-decodes-only-objects.k` | codable mutations (`codablegen`) | a JSON array decodes as an empty `Map<String, _>` instead of `mismatch` |

All three are fixed (stage 3, implementation round 2): their `.pending` files are gone and the cases guard
the fixes, and the generators no longer steer around them.
