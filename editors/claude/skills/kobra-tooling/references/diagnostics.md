# Reading and fixing diagnostics

Sources: `book/reference/diagnostics.md` (every code, with examples), `compiler/diagnostics.md`, and
the `explain_diagnostic` MCP tool.

## Format

```text
warning[K2012]: comparing a `Bool` with a literal
 --> src/m/M.k:2:12
  |
2 |     return a == true
  |            ^^^^^^^^^
  |
  = fix: Simplify to `a`

checked 5 files: 0 error(s), 1 warning(s)
```

With `k check --format json` you get one object per line:

```text
{"code":"K2012","name":"bool-literal-comparison","severity":"warning","message":"comparing a `Bool` with a literal",
 "span":{"file":"src/m/M.k","startLine":2,"startColumn":12,…},"labels":[],"notes":[],"helps":[],
 "fixes":[{"message":"Simplify to `a`","applicability":"machine-applicable","edits":[{"span":{…},"replacement":"a"}]}]}
```

A fix whose `applicability` is `machine-applicable` is safe to apply, and `k fix` applies all of them.
Others need a decision. A `note` gives context, and a `help` suggests what to do. Every code also has a
kebab-case `name`, which `explain_diagnostic` accepts too.

## Workflow

1. Fix the **first** error first. Later ones are often consequences of it.
2. Read the `fix:` and `help:` lines. They usually name the exact change.
3. Unknown code? Call `explain_diagnostic` with `K3015`, or `search_docs` with the code. Without MCP,
   search `docs/book/src/reference/diagnostics.md` for `K3015`.
4. Apply mechanical fixes with `k fix` (`--dry-run` to preview, `--code` to narrow).
5. Re-run `check_snippet` or `k check`. Warnings fail `k check`, so clear them too.

## Code ranges

| Range | Area |
|---|---|
| K0xxx | lexing and parsing (K0104 `&&`, K0107 ternary, K0103 `;`, K0129 untyped `throws`) |
| K1xxx | names, imports, modules, docs (K1201/K1202 not found, K1002 file naming, K1004 duplicate, K1302 redundant `public`, K1401 deprecated, K1901 doc link) |
| K2xxx | types, effects, control flow (K2001 type mismatch, K2011 assign to `let`, K2012 Bool literal, K2013 ordering lengths, K2201 missing `try`, K2206 non-exhaustive `catch`, K2301 non-exhaustive `match`, K2303 `case _`, K2304 unused catch-all binding, K2308 `return` in lambda) |
| K3xxx | components, views, styles, themes (K3001 `let`/`var` for an annotation, K3012/K3017 throwing effect or handler, K3014–K3016 bindings, K3027 missing page title, K3101 unknown element, K3104 attribute set twice, K3106 missing `key`, K3201 unknown CSS property, K3203 unknown class, K3204 unused class) |
| K4xxx | injection, layering, server/client boundary (K4005 component injects protocol, K4013 server-rendered read of client-only state, K4014 redundant `use`) |
| K5xxx | pages, routing, rpc (K5002 param type, K5004 endpoint needs `at`, K5014 guard on layout, K5501/K5502 rpc `throws`, K5507 guard on handler, K5508 stray `rpc`) |
| K6xxx | migrations |
| K7xxx | validation and tests (K7004 redundant `@Email`, K7503 unbound config, K7510 failed test/expectation) |
| K8xxx | lockfiles, projects, toolchains (K8001 rpc.lock, K8004 entity without migration, K8007 routes.lock, K8104 workspace cycle, K8108 preview not enabled, K8201 toolchain missing) |

## Lockfile failures

- **K8004**, an entity has no table: run `k migrate new "…"` and commit the migration plus
  `schema.lock`.
- **K8007**, routes: a renamed or removed page. Run `k build` to record the redirect, keep the old URL
  with `at "/old"`, or edit `[redirects]` in `routes.lock`.
- **K8001**, rpc wire change: revert it, or mark the protocol `@Breaking("reason")` and `k build`.
- Never hand-edit `schema.lock` or `rpc.lock`.

## Failing tests

A failed `expect` is K7510, shown with the value of each part:

```text
error[K7510]: expectation failed: cart.items.count == 1
  --> src/store/Cart.test.k:14:16
   = note: `cart.items.count` was 0
   = note: in test `Cart › adds a product`
```

"The test threw an error it didn't catch" is also K7510. The note prints the error's payload.
