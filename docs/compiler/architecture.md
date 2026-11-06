# Compiler Architecture

This document describes the architecture of the whole K toolchain: the `k` binary, the compiler, the
JavaScript runtime and the server runtime. It is the source of truth for compiler internals; the language
itself is specified in [`docs/design`](../design) and its syntax in [grammar.md](grammar.md). Related
documents: [diagnostics.md](diagnostics.md) and [testing.md](testing.md).

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
  k_cli           the `k` binary: subcommands over the crates above
  k_corpus        golden corpus harness and golden file formats (tests/corpus)
fuzz/             cargo-fuzz targets (nightly, outside the workspace)
tests/corpus/     golden corpus
```

Planned crates, added when their stage starts: `k_project` (`k.toml`, package resolution, `k.lock`) may
be split out of `k_sema`; `k_fmt` (formatter, deferred) and `k_lsp` (language server, deferred).

**Dependency rules** (arrows point to dependencies; no cycles; nothing depends on `k_cli`):

```text
k_cli ──▶ k_server ──▶ k_runtime
  │          └───────▶ k_db ──▶ k_diagnostics
  ├──▶ k_codegen ──▶ k_sema ──▶ k_syntax ──▶ k_diagnostics
  │        │           └─────▶ k_db
  │        └──▶ k_runtime
  ├──▶ k_sema (k check)
  └──▶ k_corpus (golden formats, sema suite) ──▶ k_sema, k_syntax
```

- `k_diagnostics` depends on nothing but `text-size`.
- `k_syntax` knows nothing about semantics.
- `k_sema` never generates code; `k_codegen` never reports semantic errors. Codegen may assume a checked
  program but must still not panic on an erroneous one; it refuses to emit when errors exist.
- `k_db` is low-level: it owns the schema model, a small **query IR**, SQL generation for both dialects,
  migrations and the drivers. `k_sema` lowers repository lambdas into the query IR (and reports what
  cannot be translated); `k_server` uses the drivers. The server runtime therefore never links the
  compiler.
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
| Protocols | message shapes, streaming restrictions, `protocol.lock` (11) |
| Async data | `@Load` dependency graph, `loading` required, `failed` error union (14) |
| Forms | constraint annotations, `@Form`, `Validatable` (15) |
| Persistence | translatable queries, relation loading, `schema.lock` (19) |
| Tests | `expect`, `expect throws`, `use` overrides in tests (21) |
| Metadata | typed entries, title/description warnings (22) |

### 5.6 Lockfiles

`protocol.lock`, `routes.lock` and `schema.lock` are read as inputs and compared with what the program
declares. `k check` reports breaking changes; `k build` updates the lockfiles for non-breaking additions.
Lockfiles are deterministic, sorted, line-oriented TOML so diffs review well.

---

## 6. Incremental queries

The compiler and the (deferred) language server share one engine: a memoized query database in the
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

`k_codegen` emits ES2022 modules and CSS. It never invokes an external tool.

### 7.1 Value representation

| K | JavaScript |
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
readable and tested ([§13](#13-testing-strategy)).

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
errors are decoded back into K enum values and thrown, so client `catch` stays exhaustive. Requests share
an `AbortSignal` with the effect or load that issued them.

### 8.7 Size budget

Signals, DOM renderer and hydration: at most 5 KB minified and brotli-compressed. With router and RPC
client: at most 10 KB.

---

## 9. Server runtime

`k_server` runs the server bundle in V8 through [deno_core](https://docs.rs/deno_core), with HTTP served by
[hyper](https://hyper.rs) on tokio.

### 9.1 Process model

- One tokio runtime for I/O; **N JavaScript worker threads**, each with its own `JsRuntime` (V8 isolate),
  default N = number of cores. Requests are dispatched to workers through channels; within a worker,
  requests interleave on the isolate's event loop.
- Isolates start from a **V8 snapshot** containing the runtime and the application bundle, created by
  `k build`, so cold start is fast.
- Only web-standard APIs are exposed to JavaScript (fetch, streams, URL, Web Crypto, encoding, console),
  through deno's extension crates, plus K's ops (database, secrets, logging, timers).

### 9.2 Request handling

```text
hyper request
  ├─ static asset? ──▶ dist/public, immutable caching for hashed names
  ├─ RPC path?     ──▶ RPC dispatch (§9.3)
  └─ page route?   ──▶ route match → guards (`requires`) → request-scoped DI container
                        → @Load (in-process protocol calls) → streaming SSR → hydration data
```

- Every request gets a fresh request scope: `@Scope(.request)` services and the server-side instances of
  client services, so no state leaks between users (design 10).
- SSR streams HTML: the shell and layouts flush first, then each loading boundary as its data resolves.
- A panic becomes a 500 response; in development it includes the K stack trace (mapped through source
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
  `{"value": ...}`. JSON field names are the K field names.
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

### 9.4 Configuration and secrets

`k.toml` declares the secrets an app needs (provisional syntax: `[secrets] names = ["STRIPE_KEY"]`). At
startup `dist/app` reads them from the environment (or a secrets file passed with `--secrets`) and refuses
to start if one is missing. Server code reads them through the server-only stdlib `Secrets` service, so
the compiler keeps them out of client bundles. `DATABASE_URL` selects the database (`sqlite:` or
`postgres://`).

---

## 10. Persistence

- **Schema model:** entities map to tables (snake_case names), `@Id` to the primary key, `@Unique` to
  unique indexes, `@Relation` to foreign keys or join tables.
- **Query translation:** `k_sema` lowers repository bodies from HIR into `k_db`'s query IR at compile
  time. Supported: `filter`
  with comparisons, `and`/`or`/`not`, `nil` checks, membership and string predicates; `sortedBy`
  (and descending), `limit`, `offset`, `first`, `count`, `including` (batched `IN` queries, no N+1).
  Captured values become bind parameters. SQL is generated for both dialects at compile time and embedded
  as constants. Anything else is a compile error (design 19).
- **Drivers:** SQLite through `rusqlite` (bundled SQLite, run on a blocking thread pool) and Postgres
  through `tokio-postgres` with a small connection pool. Both are exposed to JavaScript as async ops that
  take a statement id and parameters and return rows.
- **Transactions:** `db.transaction { ... }` maps to a driver transaction bound to the request; a throw
  rolls back.
- **Tests:** each integration test runs in a SQLite transaction that is rolled back (design 21).
- **Migrations:** `k migrate new <name>` diffs entities against `schema.lock` and writes
  `migrations/NNNN_<name>.sql` for both dialects; `k migrate apply` applies pending migrations and records
  them in a `k_migrations` table; destructive changes require `@Breaking("reason")`.

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
| Language server (deferred): diagnostics after a keystroke / completions | ≤ 100 ms / ≤ 50 ms at p95 |
| Compiler memory, 100,000-line project, warm | ≤ 1.5 GB |
| Client runtime size (min + brotli) | ≤ 5 KB core, ≤ 10 KB with router and RPC |
| Hydration of a 1,000-node page | ≤ 10 ms |
| Signal write to DOM text update | ≤ 1 µs per binding |
| Replacing a 1,000-row keyed list | ≤ 20 ms |
| `dist/app` cold start (snapshot) | ≤ 150 ms to first response |
| SSR of a 1,000-node page (data loaded) | ≤ 3 ms |
| RPC dispatch overhead (excluding the handler) | ≤ 50 µs at p50 |
| Trivial RPC throughput, 8 cores | ≥ 30,000 requests/s |

The syntax budgets and the `k check` budgets (cold, warm after a body or signature edit, memory) are
enforced by `scripts/bench-check.sh`; its method, the measured baselines and the profiles of the parser
and the checker are in [performance.md](performance.md). The warm body-edit budget is **not met yet**
(about 100 ms; [performance.md §10.4](performance.md#104-budget-status)): it needs the per-declaration
early cutoff of [sema.md §3.3](sema.md#33-layers).

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
  formatter idempotence, deterministic output.
- **Fuzzing** (cargo-fuzz) of lexer, parser and later the checker and codegen. Crashes become regression
  cases.
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
  k.lock               package versions and hashes
  protocol.lock routes.lock schema.lock
  App.k                `public app Name { ... }`
  src/
    shop/cart/CartService.k        module shop.cart
    shop/cart/CartService.test.k   its tests
  migrations/0001_init.sql
  dist/                build output (not committed)
    app                server executable (runtime + snapshot + server bundle)
    public/            hashed client JS, CSS and source maps
```

| Command | v0 behavior |
|---|---|
| `k check [paths]` | parse, check, lockfile comparison, tests in CI mode; `--format human\|json` |
| `k build` | check, then emit `dist/` |
| `k dev` | build and serve with rebuild on change (HMR deferred) |
| `k test` | run `test` declarations (unit, component and integration tests) |
| `k routes` | print the route table from the checked program |
| `k migrate new\|apply\|status` | migrations ([§10](#10-persistence)) |
| `k debug tokens\|ast\|diag <file>` | hidden; print golden-file formats |

Exit status: 0 success, 1 problems found, 2 usage or internal error.

`dist/app` is a copy of a slim runtime executable (the server runtime without the compiler) with the
snapshot and assets appended in an archive trailer that it reads at startup. Deployment is copying one
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
| `arbitrary` | structure-aware K program generator shared by fuzz targets and property tests (dev) | in use |
| `salsa` | incremental query engine | rejected: hand-rolled queries ([sema.md §3.1](sema.md#31-decision-hand-rolled-salsa-style-layered-queries)) |
| `serde`, `serde_json`, `toml` | lockfiles, `k.toml`, server JSON | planned |
| `tokio`, `hyper`, `hyper-util`, `http-body-util` | HTTP server | planned (k_server) |
| `deno_core` and deno web extensions | V8 embedding, web APIs | planned (k_server) |
| `rusqlite` (bundled), `tokio-postgres` | database drivers | planned (k_db) |

No JavaScript package ecosystem is used, at build time or at runtime.

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

**Deferred:** hot module replacement, the language server, `k fmt`, the Protobuf binary wire format and
`k proto export`/`import proto` code generation, the edge deployment target, packages (`k add`), client
and bidirectional streaming.

---

## 17. Open questions

- The final structure of `App.k` beyond `use` bindings and metadata (grammar RA-15). The former
  proposals (`config`, `migration`, auth and sessions, endpoint pages, ...) are now adopted in
  `docs/design/19`–`39`; the syntax layer parses `config` and `migration` (grammar RA-32), and the
  later stages and §9.4 below still need updating to match them.
- The auth and session model behind `Authenticated`, `Role` and `Session`.
- The exact secrets declaration syntax ([§9.4](#94-configuration-and-secrets)); the `config` proposal
  would replace the provisional `k.toml` list.
- Whether `Int` should be a 64-bit integer (`bigint`) on the server; v0 uses safe-integer `number`
  everywhere for identical semantics on both sides.
