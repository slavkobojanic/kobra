# `sema/control`: effects, errors and control flow

Test-first cases for [sema.md §10](../../../../docs/compiler/sema.md#10-effects) (effects: `async`,
typed `throws`, `try`, exhaustive `catch`, error-generic parameters) and
[§11](../../../../docs/compiler/sema.md#11-control-flow) (exhaustiveness, `default` rules, `guard`,
definite initialization, return paths, jumps, `for await`). Codes K2200–K2399; every span and fix
follows [diagnostics.md](../../../../docs/compiler/diagnostics.md#semantic-codes).

| Directory | Rules |
|---|---|
| `async/` | `await` placement (K2205), missing `await` (K2204), async lambdas and function values |
| `throws/` | `throw` and `try` against the declared error (K2203), missing and needless `try` (K2201, K2202) |
| `try-else/` | `try … else { }` fallbacks, and `else` belonging to `guard` |
| `catch/` | `do`/`catch` exhaustiveness (K2206), catch-alls (K2304), unreachable clauses (K2302) |
| `error-generic/` | parameters typed `() async throws E -> T` |
| `panics/` | panicking calls are not errors |
| `match/` | `match` exhaustiveness (K2301), `default`, `case _` (K2303), `case let other` (K2304), unreachable arms (K2302), `where` |
| `guard/` | `guard … else` must exit (K2305) |
| `init/` | definite initialization (K2306) and `let` assigned twice (K2011) |
| `returns/` | missing returns (K2307) |
| `jumps/` | `break`/`continue` placement (K2309), `return` in lambdas (K2308) |
| `for/` | iterating collections and ranges, `for await` over streams (K2310, K2311) |
| `recovery/` | one error must not cascade; unknown names produce no follow-on effect or flow errors |
| `projects/` | the same rules across modules |

Fix messages are the Fix column of diagnostics.md without its applicability note
(`Insert `try ` before the expression`, `Remove `try ``). Codes whose details (notes listing missing
patterns, labels on the enclosing signature) have no settled wording are matched by code and span only.
