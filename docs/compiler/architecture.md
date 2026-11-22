# Compiler Architecture

This document describes the architecture of the whole Kobra toolchain: the `k` binary, the compiler, the
JavaScript runtime and the server runtime. It is the source of truth for compiler internals; the language
itself is specified in [`docs/design`](../design) and its syntax in [grammar.md](grammar.md). Related
documents: [sema.md](sema.md), [codegen.md](codegen.md), [runtime.md](runtime.md), [server.md](server.md),
[db.md](db.md), [diagnostics.md](diagnostics.md), [testing.md](testing.md) and
[performance.md](performance.md).

Contents:

1. [Goals](#1-goals)
2. [Crate layout](#2-crate-layout)
3. [Pipeline](#3-pipeline)
4. [Syntax](#4-syntax)
5. [Semantic analysis](#5-semantic-analysis)
6. [Incremental queries](#6-incremental-queries)
7. [Code generation](#7-code-generation)
8. [JavaScript runtime](#8-javascript-runtime)
9. [Server runtime](#9-server-runtime)
10. [Persistence](#10-persistence)
11. [Diagnostics](#11-diagnostics)
12. [Performance budgets](#12-performance-budgets)
13. [Testing strategy](#13-testing-strategy)
14. [Projects, commands and build outputs](#14-projects-commands-and-build-outputs)
15. [Dependencies](#15-dependencies)
16. [Milestones](#16-milestones)
17. [Open questions](#17-open-questions)

---

## 1. Goals

1. **Correct by construction.** Every guarantee in the design (types, exhaustiveness, layering, the
   server/client boundary, reactivity rules, typed URLs) is enforced by the compiler.
2. **Never panic on input.** Every stage accepts any input and returns results plus diagnostics.
3. **Fast.** The compiler is a batch tool and a language server at once; it must be fast cold and
   instantaneous warm. Concrete budgets are in [§12](#12-performance-budgets).
4. **One binary.** `k` contains the compiler, test runner, dev server, migration tool and the server
   runtime. No Node, npm or external bundler is involved anywhere.
5. **Readable output.** Generated JavaScript is plain, debuggable ES2022 with source maps.
6. **Documented like the JDK.** Every public item has documentation that states its contract, its
   parameters, its errors and its edge cases.

---

## 2. Crate layout

```text
crates/
  k_diagnostics   Diagnostic, codes registry, spans, line index, human + JSON rendering
  k_syntax        SyntaxKind, lexer, parser, lossless rowan tree, typed AST, dumps
  k_sema          project model, name resolution, HIR, types, language rules, query database
  k_codegen       JS (client + server) and CSS generation, bundling, source maps
  k_runtime       hand-written client/shared JavaScript runtime, embedded with include_str!
  k_db            schema model, query translation, migrations, SQLite/Postgres drivers
  k_server        deno_core (V8) embedding, hyper HTTP, SSR, RPC dispatch, ops
  k_lsp           the language server: LSP transport, document sync and the IDE features over k_sema
  k_fmt           the formatter's core: canonical rewrites and layout over the lossless tree
                  (docs/design/56-formatter.md); `k fmt` itself comes later
  k_mcp           the MCP server (`k mcp`) for AI assistants: the embedded docs and examples, search,
                  diagnostic explanations and snippet checks over k_sema (mcp.md)
  k_cli           the `k` binary: subcommands over the crates above
  k_corpus        golden corpus harness and golden file formats (tests/corpus)
  xtask           `cargo xtask`: release automation (fragments, version bumps, the local gate,
                  the docs gate); never shipped, nothing depends on it (release.md)
fuzz/             cargo-fuzz targets (nightly, outside the workspace)
tests/corpus/     golden corpus
editors/          VS Code, Neovim and Vim integrations of `k lsp` (lsp.md §11)
```

Planned crates, added when their stage starts: `k_project` (`k.toml`, package resolution, `k.lock`) may
be split out of `k_sema`. The language server, `k_lsp`, is described in
[lsp.md](lsp.md).

**Dependency rules** (arrows point to dependencies; no cycles; nothing depends on `k_cli`):

```text
k_cli ──▶ k_server ──▶ k_runtime
  │          └───────▶ k_db ──▶ k_diagnostics
  ├──▶ k_codegen ──▶ k_sema ──▶ k_syntax ──▶ k_diagnostics
  │        │           └─────▶ k_db
  │        └──▶ k_runtime
  ├──▶ k_sema (k check)
  ├──▶ k_db (k migrate status)
  ├──▶ k_mcp (k mcp) ──▶ k_sema, k_diagnostics
  └──▶ k_corpus (golden formats, sema/codegen/run suites) ──▶ k_codegen, k_sema, k_syntax

k_fmt ──▶ k_syntax ──▶ k_diagnostics
```

Test-only edges: `k_server`'s tests use `k_corpus`, `k_sema` and `k_codegen` (the execution corpus and
the e2e projects need V8 and the compiler together); `k_cli`'s tests use `k_server` to build an app
archive by hand; `k_fmt`'s tests use `k_sema` and `k_corpus` to check that formatting keeps every
`sema` case's diagnostics.

- `k_diagnostics` depends on nothing but `text-size`.
- `k_syntax` knows nothing about semantics.
- `k_fmt` sees only the syntax tree: it formats one file without a project, so its rewrites are the
  ones a tree can prove.
- `k_sema` never generates code; `k_codegen` never reports semantic errors. Codegen may assume a checked
  program but must still not panic on an erroneous one; it refuses to emit when errors exist.
- `k_db` is low-level: it owns the schema model, a small **query IR**, SQL generation for both dialects,
  the migration builder, the build's database manifest and the drivers. `k_sema` reports what can't be
  translated; `k_codegen` lowers repository queries into the query IR; `k_server` uses the drivers. The
  server runtime therefore never links the compiler.
- `k_server` (and so V8) is behind the default-on cargo feature `server` of `k_cli`, so compiler
  development can build and test without compiling V8.

---

## 3. Pipeline

```text
 .k source files
      │  k_syntax::lex           (per file, parallel)
      ▼
 token stream (flat, lossless) ── lexer diagnostics
      │  k_syntax::parse
      ▼
 lossless syntax tree (rowan green tree) + typed AST views ── parser diagnostics
      │  k_sema: item tree → module tree → def maps → name resolution
      ▼
 HIR (desugared, id-based) + resolved signatures
      │  k_sema: type inference & checking, language rules
      ▼
 checked program: types, DI graph, route table, reactive graph, server/client classification
      │  k_codegen
      ├──▶ client JS modules + CSS + source maps   (browser)
      └──▶ server JS bundle                         (k_server, V8)
                 │
                 ▼
      dist/app (server executable) + dist/public (hashed client assets)
```

Each arrow is a pure function of its inputs, which is what makes the query architecture
([§6](#6-incremental-queries)) possible.

---

## 4. Syntax

### 4.1 Lexer

- Hand-written, byte-oriented, single pass with a small mode stack (code, string, interpolation, style),
  exactly as specified in [grammar.md §3–4](grammar.md#3-lexical-structure).
- Output is structure-of-arrays: `kinds: Vec<SyntaxKind>` and `starts: Vec<u32>` (plus a sentinel), so the
  parser's hot loop touches 2 bytes per token for kinds. Texts are slices of the source.
- Keyword recognition is a `match` on the identifier text (compiled to length + byte comparisons); a
  perfect hash is an option if profiling demands it.
- The lexer is independent of the parser, so any range can be relexed for incremental reparsing.

### 4.2 Parser

- Hand-written recursive descent with a Pratt loop for expressions.
- **Event-based.** The parser runs over a trivia-free view of the tokens (each carrying a
  "newline before" flag, which implements [grammar.md §5](grammar.md#5-newlines-and-termination)) and emits
  events: `Start(kind)`, `Token(n)`, `Finish`, `Error(diagnostic)`. `Start` events can be retroactively
  wrapped (`precede`) for left-recursive constructs like `BinaryExpr` and `CallExpr`, as in rust-analyzer.
- A **tree sink** turns events into a rowan green tree, re-inserting trivia. Leading trivia attaches to the
  following node, except that trailing same-line comments stay with the preceding token; `DocComment`s
  attach to the next item or member.
- **Contextual keywords** are remapped by the parser (`bump_remap`) so the tree carries `PageKw` etc.
- **Recovery.** Each list parser (items, members, statements, arguments, style items) has a recovery set
  of tokens that start the next element or close the list. Unexpected tokens are wrapped in an
  `ErrorNode` until a recovery token. A parse always consumes all tokens and always yields a
  `SourceFile`.
- **Fuel.** A step counter guards against infinite loops in the parser; exceeding it is an internal error,
  caught in debug builds by tests and fuzzing (release builds stop parsing and the sink appends the
  remaining tokens in an `ErrorNode`, so the result stays lossless).
- **Nesting limit.** Recursive constructs may nest 400 levels (grammar.md RA-34). Deeper input is K0134
  and is skipped iteratively, so the parser needs at most about 2 MiB of stack in debug builds and 1 MiB
  in release builds on any input; it fits the default 2 MiB stack of a spawned thread. Drivers that
  parse on their own threads should still give them at least 4 MiB.
- **Linearity.** Every lookahead is bounded or covers disjoint ranges (the lambda-parameter scan has no
  length cap but stops at the first `{` or `}`, so the scans of different lambdas never overlap; it uses a
  fuel-free `peek`), the helpers that compute quick-fix text cache line starts, and recovery skips each
  token once, so parsing is linear in the input even when every line has errors. `benches/syntax.rs` and
  `tests/perf_budget.rs` guard this.
- **Interpolations ended by a line break.** While building the token view, the parser replays the
  lexer's string and interpolation frames and records every interpolation that a line terminator ended
  (grammar.md §4.2). Inside such an interpolation the view ends at that line terminator (a lookahead
  *limit*: tokens past it read as end of input), so delimiters opened inside are unclosed there instead
  of swallowing the next lines.
- **Implementation map** (`crates/k_syntax/src/parser/`): `engine.rs` (token view, events, markers,
  diagnostics and recovery helpers), `items.rs` (files, imports, declarations, members, clauses),
  `statements.rs`, `expressions.rs` (the Pratt loop), `types.rs`, `patterns.rs`, `styles.rs`, and
  `sink.rs` (events to green tree).

### 4.3 Trees

- [rowan](https://docs.rs/rowan) green/red trees: green nodes are immutable and shareable across file
  versions; red nodes are created on demand with parent pointers.
- The sink builds green nodes bottom-up with `GreenNode::new` rather than rowan's `GreenNodeBuilder`.
  Tokens are interned per parse: keywords and punctuation through one slot per kind (no hashing, no text
  comparison), newlines and runs of up to 32 spaces through slots by length, and every other token of up
  to 24 bytes through a hash map keyed by kind and source slice (so lookups never allocate); nodes are
  not deduplicated. rowan's `NodeCache` rehashes cached nodes with a hash that recurses over their whole
  subtree whenever its table grows, which made long chains of small nodes (`a.b.c...`) quadratic in time
  and unbounded in stack.
- Trees can be deep without deep recursion in the parser: a 100,000-term sum nests 100,000 `BinaryExpr`
  nodes. rowan frees trees recursively, so `Parse` records the depth of its tree and, when it is deeper
  than 256 levels, tears the tree down iteratively on drop.
- Typed AST wrappers (`k_syntax::ast`) are zero-cost views generated by macros, one per node kind, plus
  enums (`Item`, `Member`, `Stmt`, `Expr`, `Type`, `Pattern`, `CssItem`).
- The S-expression dump ([grammar.md §19](grammar.md#19-syntax-tree-dump-format)) is the testing
  interface of the parser.

### 4.4 Incremental reparsing

When an edit falls entirely inside a `Block`, `MemberList`, `LambdaExpr` body or `StyleBlock` and the
relexed text of that node still has balanced delimiters, only that node is reparsed and spliced into the
green tree. Otherwise the file is reparsed (cheap: a 300-line file parses in well under a millisecond).

---

## 5. Semantic analysis

`k_sema` turns trees into a checked program. It is organized as layers of queries. **The detailed design,
including which checks are v0 and which are deferred, the stdlib surface and the diagnostic sub-ranges, is
[sema.md](sema.md)**; this section is the overview.

### 5.1 Project model and modules

- A **module** is a directory under `src/` (`src/shop/cart/` → `shop.cart`). `App.k` sits at the project
  root next to `k.toml`. Packages map their own directory trees under their module path.
- The **file rule**: at most one `public` declaration per file, named like the file. Test files
  (`*.test.k`) belong to the same module as their neighbors.
- Imports name exactly one declaration. Circular imports between modules are errors.
- **Workspaces** ([40-workspaces-and-services.md](../design/40-workspaces-and-services.md),
  [sema.md §4.1](sema.md#41-workspaces)): a root `k.toml` with `[workspace] members` groups projects.
  `k_sema::Workspace` plans the members and their dependency DAG (cycles and dependencies on apps are
  diagnostics) and gives each member's analysis its own files plus its libraries' sources mounted below
  the hidden `.deps/`, so a library's modules are imported by their own paths. Every driver (`k check`,
  `k test`, `k build`, `k dev`, the language server) goes through it; a lone project is a workspace of
  one member.

### 5.2 Item tree, def maps and name resolution

- The **item tree** of a file is a position-independent summary of its declarations and their
  signatures (names, kinds, visibility, annotations, member signatures), excluding bodies. Editing a body
  does not change the item tree, which gives early cutoff in the incremental engine.
- A **def map** per module lists every declaration visible in the module: its own declarations plus
  imports, plus the core prelude (`String`, `List`, `Color`, units, ...). Visibility (`public`,
  module-private, `private`) is checked here.
- **Name resolution** resolves paths in signatures and bodies. Implicit member expressions (`.name`) are
  resolved later, during type checking, against the expected type. The compiler-generated `Page` enum
  and the per-component `Classes` types are synthesized declarations.

### 5.3 HIR

Bodies are lowered from the syntax tree into an arena-allocated, id-based HIR: expressions, statements
and patterns with stable `ExprId`s, a source map back to syntax pointers for diagnostics, and desugaring
of `if let`, `guard let`, `try ... else`, `for` over collections, trailing lambdas, chained optional props
and string interpolation.

### 5.4 Types

- Nominal types: `struct`, `enum` (with associated data), `class`, `interface`, `service`, `protocol`,
  `entity`, `component`, plus built-ins (numbers, `String`, `Bool`, collections, `Optional`, `Result`,
  units, `Color`, `Date`, `Money`, functions, tuples).
- Bidirectional type checking with local inference: expected types flow into implicit members, lambdas,
  literals (`[]`, `[:]`, `nil`) and unit arithmetic; generic arguments are inferred by unification
  limited to call sites (no global inference).
- `where T implements I` constraints are checked at instantiation sites. Interfaces are nominal.
- **Effects:** `async` and typed `throws E` are part of function types. Every throwing call requires
  `try`; `catch` and `match` are checked for exhaustiveness with a pattern-matrix algorithm (the same one
  for both, including `where` guards, which never count toward exhaustiveness). Unreachable `default` is
  an error.
- **Units:** `Length`, `Duration`, `Percent`, `Angle` and token subtypes (`Spacing`, `Radius`, ...).
  Same-unit arithmetic folds at compile time; mixed lengths become `calc()`; mixing dimensions is an
  error.
- An erroneous expression gets the `{error}` type, which unifies with everything and suppresses cascading
  diagnostics.

### 5.5 Language rules

These checks implement the design's guarantees. Each produces diagnostics with quick fixes where the
design calls for them.

| Area | Rules (design reference) |
|---|---|
| Mutability | annotation/`let`-`var` pairing (`@State var`, `@Prop let`, ...), writes to `let` (01) |
| Reactivity | no writes to `@Derived`; no `@State` writes inside `@Derived`; effects that read nothing; reads after `await`; `@Effect(on:)` (03) |
| Components | one `view`, at most one `style`; named-vs-chained props; duplicate chained options; prop/modifier name clashes; `@Binding`/`bind` pairing; `@Slot` typing; `@Stylable` application (04) |
| Views | element/attribute typing; HTML-name hints (`a` → `link`); `key` required in view `for` loops (05) |
| Styles | property value grammars; class references exist; unused classes warn; `.class()` once; scoped hashing (06) |
| Themes | token categories, variant overrides, missing tokens (07) |
| Types/errors | exhaustiveness, `try` marking, `guard` exits, wrapping at layer boundaries (08, 09) |
| Services/DI | DI graph construction, cycles, missing/ambiguous implementations, scopes, state ownership (only a service writes its `@State`) (10) |
| Layering | who may inject what; components never inject protocols (10) |
| Server/client | classification and boundary violations ([§7.4](#74-clientserver-split)) (10, 11) |
| Routing | page enum, `@Param` types, `at` templates, URL conflicts, `routes.lock` (12) |
| Protocols | message shapes, streaming restrictions, `rpc.lock` (11, 40) |
| Async data | `@Load` dependency graph, `loading` required, `failed` error union (14) |
| Forms | constraint annotations, `@Form`, `Validatable` (15) |
| Persistence | translatable queries, relation loading, `schema.lock` (19) |
| Tests | `expect`, `expect throws`, `use` overrides in tests (21) |
| Metadata | typed entries, title/description warnings (22) |

### 5.6 Lockfiles

`routes.lock`, `schema.lock`, `storage.lock` and `rpc.lock` are read as inputs and compared with what
the program declares ([sema.md §12.16](sema.md#1216-lockfiles)). The semantic pass checks `storage.lock`
and `rpc.lock`; `k_codegen::project_diagnostics` checks `schema.lock` (with the migrations) and
`routes.lock`, which need the persistence model and the route table, and `k check` runs it for an app
without other errors. `k check` only reads lockfiles. `k build` updates `rpc.lock` (the protocols an app
serves, their wire names included: there is no separate `protocol.lock`), `routes.lock` and
`schema.lock` (the schema the migrations produce); `k migrate new` and `k migrate rename` update
`schema.lock`. Lockfiles are deterministic, sorted, line-oriented TOML so diffs review well.

---

## 6. Incremental queries

The compiler and the language server ([lsp.md](lsp.md)) share one engine: a memoized query database in the
style of [salsa](https://github.com/salsa-rs/salsa). **Decision:** the engine is hand-rolled in `k_sema`
(layered queries with revisions, early cutoff and per-layer parallel fan-out), not the `salsa` crate;
the reasons and the layers are in [sema.md §3](sema.md#3-query-architecture).

**Inputs** (set by the driver or the editor): `source_text(FileId)`, `file_set()` (paths to ids),
`project_config()` (`k.toml`), `lockfile(kind)`, `build_env()`.

**Derived queries** (pure functions of inputs and other queries):

| Query | Result | Invalidated by |
|---|---|---|
| `parse(file)` | `Parse` | that file's text |
| `item_tree(file)` | declarations and signatures | signature changes only (early cutoff on body edits) |
| `module_tree()` | modules and their files | files added, removed or moved |
| `def_map(module)` | visible names | item trees and imports of the module |
| `signature(decl)` | resolved signature | the declaration's item-tree entry, imported signatures |
| `body_hir(decl)` | HIR + source map | the declaration's syntax |
| `infer(decl)` | types of every expression | its HIR, signatures it uses |
| `check(decl)` | diagnostics | its inference, global graphs it participates in |
| `di_graph()`, `route_table()`, `server_client_classes()` | global graphs | signatures only |
| `file_diagnostics(file)` | all diagnostics of a file | the above |
| `emit_module(module, target)` | JS/CSS text | checked declarations of the module |

Design rules that keep this possible from day one, even before the query engine is adopted:

1. Every stage is a **pure function** with explicit inputs; no global mutable state, no I/O inside stages.
2. **Ids, not pointers**, across stage boundaries: `FileId`, interned `DeclId`, arena `ExprId`. Results are
   `Eq` so unchanged results stop propagation (early cutoff).
3. **Position independence:** item trees and signatures do not store offsets; diagnostics store syntax
   pointers resolved at reporting time.
4. **Cancellation:** long queries check a cancellation flag between declarations, so an editor keystroke
   can abort stale work.
5. **Parallelism:** parsing and per-declaration checking are embarrassingly parallel; the batch driver
   parses files on all cores (`std::thread::scope`), and the query engine evaluates independent queries in
   parallel.

The batch driver (`k check`) and tests use the same `AnalysisHost`; there is no separate batch path.

---

## 7. Code generation

`k_codegen` emits ES2022 modules and CSS. It never invokes an external tool. **The detailed design is
[codegen.md](codegen.md)**; this section is the summary.

### 7.1 Value representation

| Kobra | JavaScript |
|---|---|
| `Int` | `number` within the safe-integer range. Overflow and division by zero panic in debug and test builds; release builds check division by zero only. Integer `/` truncates. |
| `Double` | `number` |
| `Decimal`, `Money` | runtime `Decimal` (scaled `bigint`) and `Money` (`Decimal` + currency) |
| `Bool`, `String` | `boolean`, `string` |
| `T?` | the value, or `null` for `nil` |
| `struct` | an immutable plain object. Field mutation on a `var` compiles to a path copy (`u = {...u, address: {...u.address, city}}`), which gives value semantics without cloning on assignment. |
| `List`, `Map`, `Set` | `Array`, `Map`, `Set` with copy-on-write: a collection is mutated in place when the compiler proves the `var` is uniquely owned (not escaped), and copied on first mutation otherwise |
| `enum` | objects `{ $: tag, ...fields }` with integer tags; payload-free cases are shared frozen singletons |
| `class` | a JS class |
| units | `Duration` → milliseconds (`number`); `Angle` → degrees; `Percent` → `number`; `Length` and `Color` → CSS text, folded at compile time where possible |
| functions, lambdas | functions, arrow functions |
| typed errors | thrown as the enum value itself; panics are `KPanic` instances, rethrown by every generated `catch` |

### 7.2 Components (client)

A component compiles to a factory function over the runtime ([§8](#8-javascript-runtime)). Static DOM is
cloned from templates; each dynamic part becomes one fine-grained effect. For `Counter`:

```js
import { signal, effect, template, on, text } from "k:runtime";

const t0 = template("<div><span> </span><button>Add</button></div>");

export function Counter(props, ctx) {
  const count = signal(props.start);                 // @State var count = start
  const increment = () => { count.set(count.get() + 1); props.changed.emit(count.get()); };
  const root = t0(ctx);                              // clone, or claim during hydration
  const label = root.firstChild.firstChild;
  effect(() => text(label, `${props.label()}: ${count.get()}`));
  on(root.lastChild, "click", increment);
  return root;
}
```

- Props are getters (`props.label()`), so a parent's signal flows into the child without re-running the
  child. `@Binding` props are `{ get, set }` pairs.
- `if`, `match` and `for ... key` compile to runtime blocks (`when`, `branch`, `each`) that own a DOM range
  and a child scope.
- `@Derived` compiles to `computed`, `@Effect` to `effect`, `@Load` to an async computed with
  cancellation.
- Classes compile to hashed names: `.class([.panel, .expanded when x])` →
  `classList(el, "panel_h3k2", () => x() && "expanded_h3k2")`.

### 7.3 Server rendering

The same component also compiles to a string renderer that appends HTML chunks to a stream with no DOM
and no signals: `Counter$ssr(props, ctx, out)`. Effects do not run on the server; `@Load` fields are
awaited in dependency order, in parallel where independent. Dynamic ranges are delimited by comment
markers (`<!--[-->`...`<!--]-->`), and adjacent dynamic texts by `<!---->`, so the client can hydrate
without re-rendering.

### 7.4 Client/server split

Every declaration is classified:

- **server-only roots:** `entity`, `repository`, `service ... implements Protocol`, anything from
  `k.server`, secrets and database resources;
- **server-only** by propagation: anything that injects or references a server-only declaration;
- **shared:** structs, enums, interfaces, protocols (as contracts), pure functions;
- **client:** components, pages, layouts and client services.

Client code that reaches a server-only declaration is an error whose labels show the reference path. For
each protocol, the client bundle receives a generated RPC stub implementing it; the server bundle wires
the real implementation, which server-side rendering calls in-process.

### 7.5 Bundling, CSS and source maps

- The emitter builds the module graph itself and splits it into chunks: runtime, shared, one per page
  (loaded with dynamic `import()`), so navigation loads only what a page needs. Output file names carry a
  content hash.
- Release builds emit compact code directly (the printer omits whitespace and shortens local names); the
  embedded runtime is shipped pre-compacted by a comment/whitespace stripper.
- CSS: one rule set per component with hashed class names, theme tokens as custom properties on `:root`,
  variants as classes on `<html>`, reactive values (`{accent}`) as per-element custom properties set by
  effects. All CSS is concatenated into one hashed stylesheet per chunk.
- Source maps (v3) map JS back to `.k` files for both bundles.

---

## 8. JavaScript runtime

`k_runtime` is hand-written JavaScript (ES2022, no dependencies) embedded in the binary. It is small,
readable and tested ([§13](#13-testing-strategy)). **The detailed design is [runtime.md](runtime.md)**.

### 8.1 Signals

- `signal(v)`, `computed(fn)`, `effect(fn)`, `batch(fn)`, `untrack(fn)`, `onCleanup(fn)`.
- Push-pull with version numbers: writes mark dependents stale and schedule effects; `computed` values are
  lazy and recompute only when read and stale. Dependency sets are re-collected on every run, so
  conditional reads are tracked exactly (design 03).
- An **owner tree** (component scopes, effects) owns cleanups; disposing a scope disposes its subtree.
- **Async effects and `@Load`** receive an `AbortSignal`; a new run aborts the previous one, and RPC calls
  made during the run use that signal. Reads after the first `await` are not tracked (the compiler warns).
- **Effects are flushed in a microtask**, after the current batch, in creation order (parents before
  children).

### 8.2 Path stores

`@State` structs and collections are backed by stores that create signals lazily per accessed path. A
write to `user.address.city` notifies readers of that path and of whole-object reads of its ancestors,
but not readers of siblings (design 08). Keyed lists track per-key items and the order separately.

### 8.3 DOM renderer

Template cloning (`template()` parses HTML once into a `<template>` and clones it), direct node
references by path, and binding helpers (`text`, `attr`, `prop`, `classList`, `style`, `on`). Keyed list
reconciliation moves existing nodes using a longest-increasing-subsequence diff. Events use delegation at
the root for common bubbling events.

### 8.4 Hydration

During hydration, `template()` claims the next existing node instead of cloning and binding helpers attach
to existing nodes; comment markers delimit dynamic ranges. Serialized data
(`<script type="application/json" id="k-data">`) holds `@Load` results and request-scoped service state
keyed by component instance path, so nothing is fetched twice. A mismatch between server HTML and the
client's expectation in development reports a precise error and falls back to client rendering of that
range.

### 8.5 Router

The generated `Page` enum carries typed parameters; generated `href` and `parse` functions per page come
from `routes.lock`. Navigation uses the History API, intercepts internal links, loads the page chunk,
runs `@Load`, keeps mounted layouts, and restores scroll position. Guards run on the server for every
request; on the client they only decide whether to navigate or redirect, never to protect data.

### 8.6 RPC client

Generated stubs call `rpc(path, message, signal)`: `POST` with JSON bodies ([§9.3](#93-rpc)). Typed
errors are decoded back into Kobra enum values and thrown, so client `catch` stays exhaustive. Requests share
an `AbortSignal` with the effect or load that issued them.

### 8.7 Size budget

Signals, DOM renderer and hydration: at most 5 KB minified and brotli-compressed. With router and RPC
client: at most 10 KB. The browser's service support (`services.js`: component-scope `provide`, `Clock`,
`Theme`, `BrowserStorage`, `FetchHttpClient`, the `Session` mirror) has its own 3 KB, like the stdlib
bodies. Features only some apps use ship as chunks of their own, only to those apps, each with its own
budget: `k.charts` 3 KB, server streams in the browser 1 KB. The budget test measures each part as it
ships (the links between runtime files removed, compacted) with gzip at the best level, which is an upper
bound for brotli ([runtime.md §2](runtime.md#2-modules-api-and-size-budget)).

---

## 9. Server runtime

`k_server` runs the server bundle in V8 through [deno_core](https://docs.rs/deno_core), with HTTP served by
[hyper](https://hyper.rs) on tokio. **The detailed design is [server.md](server.md)**.

### 9.1 Process model

- One tokio runtime for I/O; **N JavaScript worker threads**, each with its own `JsRuntime` (V8 isolate),
  default N = number of cores. Requests are dispatched to workers through channels; within a worker,
  requests interleave on the isolate's event loop.
- Isolates start from a **V8 snapshot** containing the runtime and the application bundle, created by
  `k build`, so cold start is fast.
- **Decision:** no deno extension crates. Isolates get what V8 provides plus a small bootstrap
  (`console`, `queueMicrotask`, `AbortController`, `TextEncoder`/`TextDecoder`) and Kobra's ops (logging,
  requests, database, config, timers, crypto, fetch); there is no ambient I/O
  ([server.md §3–4](server.md#3-process-model)). This keeps the binary and the attack surface small and
  every effect on the world visible as an op.

### 9.2 Request handling

```text
hyper request
  ├─ static asset? ──▶ dist/public, immutable caching for hashed names
  ├─ RPC path?     ──▶ RPC dispatch (§9.3)
  └─ page route?   ──▶ route match → guards (`requires`) → request-scoped DI container
                        → @Load (in-process protocol calls) → streaming SSR → hydration data
```

- Every request gets a fresh request scope: `@Scope(.request)` services and the server-side instances of
  client services, so no state leaks between users (design 10). State every worker must share (sessions,
  rate limits, `@Scope(.server)` state, broadcasts) lives in Rust behind ops; sessions and rate-limit
  windows are rows of the app's database when it has one, so replicas share them
  ([server.md §9](server.md#9-sessions-auth-and-request-forgery)).
- Background jobs run in the same process: a jobs worker claims due rows of `k_job` and hands each run
  to a worker isolate like a request ([server.md §11](server.md#11-background-jobs)). Outgoing HTTP and
  SMTP go through ops (`op_k_fetch`, `op_k_smtp_send`; [server.md §18](server.md#18-outgoing-http-and-email)).
- **Decision (round 1b):** after the Rust front's checks, a request is handled by the hand-written
  `k:runtime/app` over a table the generated server entry describes (routes, rpcs, endpoints,
  containers); see [server.md §1](server.md#1-status-and-scope).
- SSR streams HTML: the shell and layouts flush first, then each loading boundary as its data resolves.
  (v0 renders the whole document, then sends it.)
- A panic becomes a 500 response; in development it includes the Kobra stack trace (mapped through source
  maps), in production a request id only.

### 9.3 RPC

v0 uses **JSON over HTTP with Connect-compatible paths and errors**; the binary Protobuf wire format is
deferred.

- **Path:** `POST /{module}.{Protocol}/{Rpc}` with the rpc name in PascalCase, for example
  `/shop.cart.CartProtocol/Add`.
- **Request body:** if the rpc has exactly one parameter whose type is a struct, that struct is the
  message; otherwise a generated message whose fields are the parameters (an rpc without parameters
  takes `{}`). `Content-Type: application/json`.
- **Response body:** a struct return value is the message; any other return type is wrapped as
  `{"value": ...}`. JSON field names are the Kobra field names.
- **Errors** use the Connect error format, `{"code": "...", "message": "...", "details": [...]}`:

  | Cause | Connect code | HTTP |
  |---|---|---|
  | typed error thrown by the rpc | `failed_precondition`, detail `{"type": "k.TypedError", "debug": {"error": "shop.cart.CartError", "case": "outOfStock", "fields": {"available": 3}}}` | 400 |
  | `Validatable` request invalid | `invalid_argument`, detail `k.ValidationError` with field paths | 400 |
  | `Authenticated` guard fails | `unauthenticated` | 401 |
  | `Role` guard fails | `permission_denied` | 403 |
  | `RateLimit` guard fails | `resource_exhausted` | 429 |
  | panic | `internal` | 500 |
  | unknown rpc | `unimplemented` | 404 |

- Dispatch: the path selects a generated handler, which validates the message, runs the guards, creates
  the request scope and calls the implementation. During SSR, protocol calls skip HTTP entirely.
- Server streaming (`Stream<T>`) uses Connect's streaming envelope with JSON messages (after the v0 unary
  slice).
- **Between services** ([40-workspaces-and-services.md](../design/40-workspaces-and-services.md)) the
  same wire format: a `remote` rule's client posts from the server with `op_k_fetch`, signs each call
  with a short-lived service token (`x-k-service-token`), and the serving app's `ServiceAuthenticated`
  guard verifies it (`runtime/remote.js`; [runtime.md §8.1](runtime.md#81-remote-clients)).

### 9.4 Configuration and secrets

Configuration and secrets are typed `config` declarations read from the environment
([25-config-and-secrets.md](../design/25-config-and-secrets.md)): `NAME_FILE` (secrets only), then
`NAME`, then the default; every problem is reported at once and `dist/app` exits 78 before opening a port;
secret values are never printed. The compiler keeps configs and secrets out of client bundles.
`DATABASE_URL` selects the database (`sqlite:` or `postgres://`). Details:
[server.md §8](server.md#8-configuration-and-secrets).

---

## 10. Persistence

**The detailed design is [db.md](db.md)**; this section is the summary.

- **Schema model:** entities map to tables (snake_case names, or the names `schema.lock` records),
  `@Id` to the primary key, `@Unique` to unique indexes, a to-one `@Relation` to a `<field>_id` foreign
  key, a to-many one to its target's foreign key.
- **Query translation:** `k_sema` checks repository lambdas (K6001 for calls SQL can't express);
  `k_codegen` (`crate::db`) lowers every awaited chain on a repository's collection into `k_db`'s query
  IR. Supported: `filter` with comparisons, `and`/`or`/`not`, `nil` checks, membership, `contains`/
  `startsWith`/`endsWith`; `sortedBy`/`thenBy` (and descending), `limit`, `offset`, `first`, `single`,
  `count`, bulk `update`/`delete`, `including` (one query per relation, no N+1). Every value is a bind
  parameter. SQL is rendered for both dialects at build time into the build's **database manifest**
  (numbered statements, migrations, unique indexes), never into JavaScript. Anything else fails the
  build (design 19).
- **Drivers:** SQLite through `rusqlite` (bundled SQLite, on a blocking thread pool) and Postgres
  through `tokio-postgres`, behind one `Database`; the server runs it on its own `k-db` runtime and
  exposes it to JavaScript as async ops that take a statement number and parameters.
- **Transactions:** `database.transaction { … }` is a driver transaction bound to the request, nested
  blocks are savepoints, a throw rolls back. v0 has one connection per process, leased to a transaction
  for its duration.
- **Migrations:** `migrations/NNNN_*.k` hold `Schema` builder calls, which `k_db::builder` replays into
  both dialects at build time; the build fails when a migration doesn't replay or the result lacks an
  entity's columns. `dist/app serve` applies the pending ones before the port opens and records them in
  `k_migrations`. `k migrate new` (diffing entities against `schema.lock`) is planned.
- **Tests:** `tests/corpus/db` serves apps on fresh SQLite (and Postgres) databases; `k test`'s
  rolled-back transaction per test is planned (design 21).

---

## 11. Diagnostics

The diagnostic model and its rules are specified in [diagnostics.md](diagnostics.md). Architecturally:

- `k_diagnostics::Diagnostic` is the only error type for user problems in every stage; internal invariant
  failures are bugs, never diagnostics.
- Stages return `(result, Vec<Diagnostic>)`; they never stop at the first error. Semantic analysis runs on
  trees with errors, using `{error}` types to avoid cascades.
- Spans are byte ranges in a file; positions are rendered as 1-based line and character column.
- Output: human text (rustc style, with snippets) for terminals, and JSON Lines (`--format json`) for
  agents and editors. Quick fixes carry exact text edits and an applicability.
- Codes are stable and registered centrally; each stage owns a range.

---

## 12. Performance budgets

Budgets are measured on a recent laptop-class CPU (Apple M-series or equivalent) in release builds.
Benchmarks ([testing.md §9](testing.md#9-benchmarks-and-performance-budgets)) track each one; a regression
past a budget blocks the change.

| Area | Budget |
|---|---|
| Lexing | ≥ 150 MB/s single-threaded |
| Lexing + parsing to a green tree | ≥ 25 MB/s single-threaded; a typical 300-line file in ≤ 0.5 ms |
| Lexing + parsing pathological input (nesting at and past the limit, 1 MB lines, huge interpolations, an error on every line) | ≥ 10 MB/s single-threaded, and linear in the input size |
| Cold `k check`, 100,000-line project (≈ 3.5 MB, ≈ 1,000 files) | ≤ 1.0 s on 8 cores, ≤ 4 s on 1 core |
| Warm re-check after editing a function body | ≤ 30 ms |
| Warm re-check after changing a public signature | ≤ 150 ms |
| Cold `k build`, 100,000-line project | ≤ 2.5 s, excluding a cached V8 snapshot |
| Language server: diagnostics after a keystroke / completions | ≤ 100 ms / ≤ 50 ms at p95; hover and go-to-definition ≤ 2 ms at p50 warm ([performance.md §12](performance.md#12-language-server)) |
| Compiler memory, 100,000-line project, warm | ≤ 1.5 GB |
| Client runtime size (min + brotli) | ≤ 5 KB core, ≤ 10 KB with router and RPC; the shipped runtime chunk (with the stdlib bodies, ≤ 10 KB of their own, and the browser services, ≤ 3 KB of their own) ≤ 23 KB, the sum of its parts; the feature chunks ≤ 3 KB (`k.charts`), ≤ 4 KB (`k.drag`), ≤ 1 KB (`k.device` files), ≤ 512 B (`KeyboardShortcut`), ≤ 1.5 KB (`HttpResponse`) and ≤ 1 KB (server streams) |
| Hydration of a 1,000-node page | ≤ 10 ms |
| Signal write to DOM text update | ≤ 1 µs per binding |
| Signal write propagated through computeds to an effect | ≤ 1 µs per write (a diamond of three computeds) |
| Replacing a 1,000-row keyed list | ≤ 20 ms; 10,000 rows ≤ 200 ms (linear); every other list operation of 1,000 rows ≤ 20 ms |
| `dist/app` cold start (snapshot) | ≤ 150 ms to first response |
| Code generation (lowering + printing), single-threaded | ≥ 200,000 lines/s ([codegen.md §20](codegen.md#20-performance-budgets)) |
| SSR of a 1,000-node page (data loaded) | ≤ 3 ms |
| SSR under load (2 clients per core), the 1,000-node page and the example apps' pages | ≥ 1,000 pages/s per core, p99 ≤ 25 ms |
| RPC with one SQLite query ([db.md §14](db.md#14-performance-budgets)) | ≤ 250 µs at p50, whole round trip |
| RPC dispatch overhead (excluding the handler) | ≤ 50 µs at p50 |
| Trivial RPC throughput, 8 cores | ≥ 30,000 requests/s |

The language server's budgets are enforced by `scripts/bench-check.sh --lsp` through
`crates/k_lsp/tests/perf_budget.rs` ([performance.md §12](performance.md#12-language-server)).

The code generation, runtime, server and `dist/app` budgets are enforced by `scripts/bench-check.sh`
(`--codegen`, `--runtime`, `--server`, `--db`, `--app`) through `crates/k_codegen/tests/perf_budget.rs`,
`crates/k_runtime/tests/runtime.rs`, `crates/k_server/tests/perf_budget.rs`, `crates/k_db/tests/perf_budget.rs`
and `crates/k_cli/tests/perf_budget.rs` (the real `dist/app` of the examples over HTTP); method and baselines
are in [performance.md §11](performance.md#11-stage-3-runtime-and-server). All are met today; the full client
runtime has 462 bytes of gzip headroom and the stdlib bodies 403
([performance.md §11.3](performance.md#113-budget-status)). The SSR-under-load and 10,000-row budgets were added with the stage 3
benchmarks: the first is the 3 ms single-request budget turned into a per-core rate with margin, the second
the 1,000-row budget scaled linearly.

The syntax budgets and the `k check` budgets (cold, warm after a body or signature edit, memory) are
enforced by `scripts/bench-check.sh`; its method, the measured baselines and the profiles of the parser
and the checker are in [performance.md](performance.md). The warm body-edit budget is met with little
margin (28.7 ms of 30 ms on 2026-11-07; [performance.md §10.4](performance.md#104-budget-status)); the
per-declaration early cutoff of [sema.md §3.3](sema.md#33-layers) would widen it.

Practices that keep the compiler within budget: structure-of-arrays token storage, interning of names,
arena allocation for HIR, no per-token heap allocation, `&str` slices instead of `String`s in hot paths,
parallel parsing and checking, and early cutoff in the query engine.

---

## 13. Testing strategy

All test kinds, their commands and their file formats are documented in [testing.md](testing.md). Summary:

- **Unit tests** in every module; **integration tests** per crate.
- **Golden corpus** (`tests/corpus`): token streams, S-expression trees and diagnostics now; checks,
  generated JS/CSS later. Expectations are written by hand, before the implementation.
- **Property tests** (proptest) for invariants: losslessness, never panicking, dump round-trips,
  formatter idempotence, deterministic output; and **differential tests** of stage 3: generated programs
  run in V8 against a reference evaluator, release against development builds, server HTML hydrated
  against the client render, the JSON wire format against a reference codec, SQLite against a reference
  query evaluator ([testing.md §5.6](testing.md#56-stage-3-properties-code-generation-execution-the-server-and-sql)).
- **Fuzzing** (cargo-fuzz) of lexer, parser, checker, code generation (and execution in V8), RPC JSON
  decoding and HTTP request handling through the real server. Crashes become regression cases.
- **Regression tests** for every fixed bug.
- **End-to-end tests:** the `k` binary on projects; built servers over HTTP; real browsers through the
  DevTools protocol when available; the example apps.
- **Runtime tests:** the JavaScript runtime is tested inside the embedded V8 (`k_server` test harness)
  against a minimal in-memory DOM written for testing, plus browser e2e for real-DOM behavior. No Node.
- **Benchmarks** (criterion) for every budget in [§12](#12-performance-budgets).

---

## 14. Projects, commands and build outputs

```text
my-app/
  k.toml               name, version, dependencies, aliases, secrets, database
  k.lock               the toolchain that last built it (later also package versions and hashes)
  routes.lock schema.lock storage.lock rpc.lock
  App.k                `public app Name { ... }`
  src/
    shop/cart/CartService.k        module shop.cart
    shop/cart/CartService.test.k   its tests
  migrations/0001_init.k
  dist/                build output (not committed)
    app                server executable (runtime + snapshot + server bundle)
    public/            hashed client JS, CSS and source maps
```

| Command | v0 behavior |
|---|---|
| `k check [paths]` | parse, check, lockfile comparison, tests in CI mode; `--format human\|json` |
| `k build [project] [--out dir]` | check, then emit `dist/` ([codegen.md §19](codegen.md#19-output-layout-of-dist)) and update `rpc.lock`, `routes.lock`, `schema.lock` and `k.lock`'s `toolchain`; hashed assets of earlier builds are removed from `public/assets/` |
| `k run [project] [--port n]` | build for development and serve in-process |
| `k dev` | build and serve with rebuild on change (HMR deferred) |
| `k test [project] [--filter text]… [--watch]` | run `test` declarations in one isolate over the fake DOM, ([codegen.md §21.1](codegen.md#211-the-test-bundle-k-test), [runtime.md §13](runtime.md#13-testing)); one browser and one server per test (rpcs in-process through the wire format, a fresh in-memory database, the test's one session); failures are K7510 diagnostics; `--watch` re-runs the tests a change affects (`Analysis::file_dependencies`); `K_TEST_BUNDLE_DIR` writes the bundle ([testing.md §8.4](testing.md#84-k-test-and-the-project-commands)) |
| `k routes [project]` | print the route table: pages, each endpoint handler, `routes.lock` redirects, with parameters and guards (`k_codegen::route_table`) |
| `k migrate new "description" [--empty \| --drop-retired \| --rename name…]`, `k migrate rename old new`, `k migrate apply\|status [project]` | migrations ([§10](#10-persistence), [db.md §10](db.md#10-migrations)) |
| `k debug tokens\|ast\|diag <file>` | hidden; print golden-file formats |

Exit status: 0 success, 1 problems found, 2 usage or internal error.

Before a project command runs (`k lsp` included), `k_cli`'s `toolchain` module reads the project's `k`
requirements and `k.lock` and, when the running `k` doesn't match, runs the installed toolchain that does
(`$K_HOME/toolchains/<version>/k`, with `K_TOOLCHAIN_DELEGATED=1`) or stops with `K8201`
([48-releases-and-versioning.md](../design/48-releases-and-versioning.md#when-the-installed-k-doesnt-match)).
`K_TOOLCHAIN=<version>` chooses for one command.

`dist/app` is a copy of a runtime executable with the server bundle and assets appended in an archive
trailer that it reads at startup ([server.md §12](server.md#12-packaging-distapp)); in v0 the runtime
executable is `k` itself, which serves when it finds a trailer, and a slim runtime binary without the
compiler is deferred. Deployment is copying one
file, or a minimal container image (`FROM scratch`-style with CA certificates).

---

## 15. Dependencies

Few, well-known crates. Adding one requires a line here.

| Crate | Use | Status |
|---|---|---|
| `rowan`, `text-size` | lossless syntax trees, text ranges | in use |
| `clap` | command-line parsing | in use |
| `libtest-mimic` | per-case corpus tests (dev) | in use |
| `proptest` | property tests (dev) | in use |
| `criterion` | benchmarks (dev) | in use |
| `libfuzzer-sys` | fuzz targets (nightly, outside workspace) | in use |
| `arbitrary` | structure-aware Kobra program generator shared by fuzz targets and property tests (dev) | in use |
| `salsa` | incremental query engine | rejected: hand-rolled queries ([sema.md §3.1](sema.md#31-decision-hand-rolled-salsa-style-layered-queries)) |
| `serde`, `serde_json` | manifests, ops, RPC JSON, app archive, `k test` results, `k mcp`'s JSON-RPC (no MCP SDK: [mcp.md §2](mcp.md#2-dependencies)) | in use (k_codegen, k_server, k_db, k_corpus, k_cli, k_mcp) |
| `toml` (parse only) | `schema.lock`; `storage.lock` (the payload check); `fakes.toml` of e2e projects (dev) | in use (k_db, k_sema; k_server tests) |
| `yaml-rust2` (no default features) | reading `examples/shop`'s `docker-compose.yml` and Kubernetes manifests in `k_cli`'s `tests/deploy.rs` (dev); pure Rust, YAML 1.2 | in use (k_cli tests) |
| `tokio`, `hyper`, `hyper-util`, `http-body-util`, `bytes` | HTTP server, async drivers | in use (k_server, k_db) |
| `deno_core`, `deno_error` | V8 embedding, ops | in use (k_server) |
| deno web extensions | web APIs in isolates | rejected: a small bootstrap and Kobra ops instead ([§9.1](#91-process-model)) |
| `rusqlite` (bundled, statement cache, `collation` for `k_decimal`), `tokio-postgres` | database drivers | in use (k_db) |
| `flate2` | gzip for the runtime size-budget test (dev); the gzip and deflate streams of toolchain archives for `k toolchain install` (`k_cli` `archive.rs`, whose tar and zip readers are its own); compressing `k mcp`'s embedded docs at build time and inflating them on first use ([mcp.md §4](mcp.md#4-the-corpus)) | in use (k_runtime, k_cli, k_mcp) |
| `getrandom` | the OS CSPRNG for session ids and OAuth state (already in the tree through deno_core) | in use (k_server) |
| `argon2` | Argon2id password hashing (`k.crypto.Password`, PHC strings; RustCrypto, pure Rust) | in use (k_server) |
| `hyper` (`client`), `hyper-util` (`client-legacy`) | outgoing HTTP for `FetchHttpClient` (`op_k_fetch`), the same HTTP stack as the server; `k_server::outbound::download`, which `k toolchain install` uses for the index and archives | in use (k_server) |
| `hyper-rustls`, `rustls` (`ring` provider, `webpki-roots`) | TLS for outgoing HTTPS and SMTP, pure Rust, no system certificates or OpenSSL needed in the container | in use (k_server) |
| `lettre` (`smtp-transport`, `tokio1-rustls-tls`, `builder`) | `SmtpEmailProvider`: SMTP with STARTTLS/TLS/plain, authentication, MIME building and reply classification (`op_k_smtp_send`) | in use (k_server) |
| `oxc_allocator`, `oxc_ast`, `oxc_parser`, `oxc_span`, `oxc_syntax`, `oxc_minifier`, `oxc_codegen` (`=0.155.0`) | minifying, tree-shaking and source-mapping an app's release client in process (k_codegen `minify.rs`, `link.rs`; [codegen.md §17.2](codegen.md#172-minification)); pinned exactly, since the output is part of every content hash. About 150 crates in the tree, 3 MB of `k`. Raised the workspace's `rust-version` to 1.97, oxc's minimum; `clippy.toml`'s `msrv` is kept equal to it, so clippy applies 1.97 lints (let chains, `as_chunks`) and never warns that the two differ. Closure Compiler was rejected (a JVM, ~1.2 s per app; [13-tooling.md](../design/13-tooling.md#commands)) | in use (k_codegen) |
| `ring` | `k.crypto`'s key types on the server (`k_server` `crypto.rs`): AES-256-GCM, HKDF, PBKDF2, Ed25519 and ECDSA P-256; already in the tree as rustls's provider, so no new code in the binary | in use (k_server) |
| `x25519-dalek` (`static_secrets`) | static X25519 keys for `X25519PrivateKey` (`ring`'s agreement keys are ephemeral only); RustCrypto, like `sha2` and `argon2`, over `curve25519-dalek` | in use (k_server) |
| `p256` (`arithmetic`) | the public point of an imported raw P-256 scalar, so `ring` can build the key pair from both halves; RustCrypto | in use (k_server) |
| `sha2` | SHA-256 of session tokens and OAuth state in `k_session`, and HMAC-SHA-256/512 (`op_k_hmac`, RFC 2104 over `sha2`) | in use (k_server) |
| `tokio-tungstenite`, `tungstenite` (`=0.29.0`, `handshake` only), `futures-util` (`sink`) | the channels' WebSocket at `/_k/channels` (k_server `sockets.rs`, [44-channels.md](../design/44-channels.md)): RFC 6455 framing over hyper's upgraded connection, and the raw client of the channel tests; pinned exactly, since the wire depends on it. The upgrade itself is hyper's | in use (k_server, k_server tests) |

| `lsp-types` (with `url`) | the Language Server Protocol's data types in `k_lsp`; the transport is our own ([lsp.md §1](lsp.md#1-dependencies)) | in use (k_lsp, k_cli tests) |
| `lsp-server`, `tower-lsp` | LSP transport | rejected: the first ends the connection on a malformed body, the second needs an async runtime the synchronous analysis doesn't want ([lsp.md §1](lsp.md#1-dependencies)) |
| `regex` (dev) | evaluates the TextMate grammar's patterns against the lexer's keyword table and token stream (`k_lsp/tests/editors.rs`, `k_lsp/tests/textmate.rs`) | in use (k_lsp tests) |

No JavaScript package ecosystem is used, at build time or at runtime. The VS Code extension in
`editors/vscode` depends on `vscode-languageclient` through npm; it is not part of the toolchain's build or
tests.

---

## 16. Milestones

**v0: the full-stack vertical slice.**

- Core language: all declarations and statements in [grammar.md](grammar.md), type checker with
  exhaustiveness, typed throws, units and extensions.
- Components, views, styles (scoped CSS, themes), reactivity (signals, effects, `@Load`), slots, bindings,
  `@Stylable`.
- Services and compile-time DI with scopes; layering and server/client boundary checks.
- Pages, layouts and routing with typed URLs, guards and `routes.lock`.
- Protocols over JSON/HTTP (Connect-compatible paths and errors), unary first.
- Entities and repositories on SQLite and Postgres, migrations and `schema.lock`.
- SSR with hydration; `k check`, `k build`, `k dev` (without HMR), `k test`, `k routes`, `k migrate`.

**Deferred:** hot module replacement, the `k fmt` command (its core, `k_fmt`, exists), the Protobuf binary wire format and
`k proto export`/`import proto` code generation, the edge deployment target, packages (`k add`), client
and bidirectional streaming.

---

## 17. Open questions

- The final structure of `App.k` beyond `use` bindings and metadata (grammar RA-15). The former
  proposals (`config`, `migration`, auth and sessions, endpoint pages, ...) are now adopted in
  `docs/design/19`–`39`; the syntax layer parses `config` and `migration` (grammar RA-32), and the
  later stages and §9.4 below still need updating to match them.
- The auth and session model behind `Authenticated`, `Role` and `Session`.
- Whether `Int` should be a 64-bit integer (`bigint`) on the server; v0 uses safe-integer `number`
  everywhere for identical semantics on both sides.
