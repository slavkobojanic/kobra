# Code Generation

This document specifies `k_codegen`: how the checked program becomes JavaScript modules, CSS and the build
manifest, how the client/server split is guaranteed, and what `dist/` contains. It is the source of truth
for lowering decisions; the runtime the generated code calls is [runtime.md](runtime.md), the server that
runs it is [server.md](server.md), persistence is [db.md](db.md). The architecture overview is
[architecture.md §7](architecture.md#7-code-generation).

Contents:

1. [Status and scope](#1-status-and-scope)
2. [Public API](#2-public-api)
3. [Pipeline](#3-pipeline)
4. [Names and modules](#4-names-and-modules)
5. [Value representation](#5-value-representation)
6. [Expressions and statements](#6-expressions-and-statements)
7. [Errors, panics and typed throws](#7-errors-panics-and-typed-throws)
8. [Async and cancellation](#8-async-and-cancellation)
9. [Generics, interfaces and witnesses](#9-generics-interfaces-and-witnesses)
10. [Components on the client](#10-components-on-the-client)
11. [Views: DOM creation, SSR emission, hydration markers](#11-views-dom-creation-ssr-emission-hydration-markers)
12. [Styles, themes and variants](#12-styles-themes-and-variants)
13. [Services and dependency injection](#13-services-and-dependency-injection)
14. [Routing tables](#14-routing-tables)
15. [RPC: client stubs and server dispatch](#15-rpc-client-stubs-and-server-dispatch)
16. [The client/server split](#16-the-clientserver-split)
17. [Bundling, chunks and printing](#17-bundling-chunks-and-printing)
18. [Source maps](#18-source-maps)
19. [Output layout of `dist/`](#19-output-layout-of-dist)
20. [Performance budgets](#20-performance-budgets)
21. [Testing](#21-testing)

---

## 1. Status and scope

The crate's API is final and implemented: the core language (§4–§9), components, views, styles, themes,
services, `@Load`, `@Form`, routing tables, RPC client stubs, repositories (§13.1, round 1c) and
the client/server split (§10–§17) are lowered (round 1a), and so are the server entry (round 1b): the DI
containers, the typed route table and redirects, the rpc dispatch table with codecs, validation and
guards, the endpoint table with request binding, metadata, `failed` blocks on the server, configs read
from startup values, and the manifest's endpoint and config tables. Every case of `tests/corpus/run`,
`tests/corpus/codegen` and `tests/e2e` passes. The browser's DI containers (§13) are lowered too, and the
four example apps run in the browser tests ([testing.md §4.3](testing.md#43-browser-tests)): server-
rendered, hydrated without a mismatch, and driven through their main interactions. The server binds the
stdlib's platform services to their server bodies (§13.2), background jobs get their kinds, spec table
and manifest entries (§13.3), and in-process rpc calls run their guards and validation (§15). Source
maps (§18) are still open.

**Inputs:** lowering reads the checked program through `Analysis::program()` (the program view),
`type_info_at` and `resolve_at` ([sema.md §2](sema.md#2-public-api)) and walks the syntax trees of
`Analysis::parse`; it has no second checker. Module layout (`crates/k_codegen/src/`): `js.rs` the JS IR
and printer, `cx.rs` the context (builtins, names, types), `lower.rs` bodies, `native.rs` stdlib calls,
`decl.rs` declarations, `codec.rs` codecs, `ui.rs` components and services, `view.rs` views, `css.rs`
styles, `app.rs` pages, routes, loads, forms and rpc, `server.rs` the server entry, `jobs.rs` background jobs, `db.rs` entities, repositories, queries and
migrations ([db.md](db.md)), `bundle.rs` files.

**Decisions taken while implementing** (each reflected in its section): struct and enum values become
an interface existential as an instance of a generated adapter class `S$as$I` whose methods call the
module functions, so interface calls are plain method calls on boxes and class instances alike (§9);
witness tables for interface-constrained generics are `S$w$I` (and `I$dyn` for classes); text functions,
codecs, adapters and witnesses are emitted only when referenced; `T??` is not distinguished by `k_sema`,
so reading a `nil` element of a list of optionals yields `$k.NONE`, which prints as `nil` (§5);
compiler-generated markup (templates, `out.raw`) is printed as an HTML string literal that keeps comment
markers, while user strings break up `<!--` (§3); the client bundle is `runtime`, `shared`,
`pages/<page>` and `app` chunks (§17).

**v0** (the full-stack vertical slice of [architecture.md §16](architecture.md#16-milestones)): everything in
sections 4–19 unless marked *deferred*. **Deferred:** hot module replacement, the binary Protobuf
encoding, client and bidirectional streaming, code splitting below page granularity, CSS minification
beyond whitespace removal, `@Rendering` per page, and the release-build name shortener (release builds
in v0 strip whitespace and comments only).

Rules that hold for every part:

- **Codegen never reports semantic errors.** It runs only on a program with no error diagnostics and
  must not panic on any program (it returns `Unsupported` for what it can't lower).
- **Deterministic output.** The same program and options produce byte-identical files: declarations are
  emitted in a fixed order (module path, then source order), hashes are content hashes, nothing depends
  on the clock, the environment or hash-map iteration order.
- **Readable output.** Development output is indented ES2022 that maps line-for-line onto the K source
  where possible; generated helper names start with `$` so they never collide with K names.

---

## 2. Public API

`crates/k_codegen/src/lib.rs`:

| Item | Purpose |
|---|---|
| `generate(&Analysis, &Options) -> Result<Output, CodegenError>` | The only entry point. |
| `Options { profile, environment, entry }` | `Profile::{Development, Release, Test}`; `Environment::{Production, Development, Test}` selects `use … in` rules; `Entry::{App, Main(name)}`. |
| `Output { client, server, server_entry, manifest, database }` | Client files (JS, CSS, maps; URL paths below `/assets/`), server modules (specifiers `k:/app/…`), the entry module, the [manifest](#19-output-layout-of-dist), and the database manifest (`k_db::manifest::DbManifest`: every statement by number in both dialects, the migrations, the unique indexes; `None` without persistence, [db.md §6](db.md#6-drivers-the-manifest-and-ops)). |
| `Manifest { name, rpc_paths, routes, endpoints, config, static_only }` | What the server front and `dist/app` need without reading code: rpc paths from the protocols with an implementation, endpoints with their methods and `uses_session`, and one `ConfigEntry { env, declared_at, ty, secret, required, default, cases, range }` per `@Env` field the production graph reads (the project's configs, `k.server.ServerConfig`, and `SessionConfig`/`DatabaseConfig`/`JobConfig` when the app signs users in, persists or has jobs; read from the embedded stdlib's syntax). |
| `CodegenError::{HasErrors { count }, Unsupported { what }, Cancelled}` | Why nothing was produced. |
| `check_client_bundle(&Output, forbidden) -> Vec<SplitViolation>` | The independent split check ([§16](#16-the-clientserver-split)). |
| `hash::{fnv1a, content_hash, hashed_file_name, scope_suffix, scoped_name}` | File-name and CSS-name hashing. |
| `dist::write_public(&Output, dist)` | Writes `dist/public/assets/*` and `dist/manifest.json`. |

`Profile::Test` prints like `Development` but uses placeholder hashes (`app.[hash].js`) and readable class
names (`CartSummary__panel`), and omits source-map comments, so golden files change only when the code
does. `Entry::Main("corpus.main")` makes the server entry module export `run()`, an async function that
calls `main()` and returns the result line of the [execution corpus](testing.md#32-the-run-suite).

---

## 3. Pipeline

```text
Analysis (checked)
  │ 1. collect: declarations, their side (server-only / client-only / shared), DI graph per environment,
  │    route table, protocol table, entity schema, theme
  │ 2. reach: the server roots and the client roots; the reachable declarations of each side
  │ 3. lower: each reachable declaration → JS IR (a small typed JS AST) per side
  │ 4. styles: style blocks + theme → CSS rules
  │ 5. bundle: modules → chunks; resolve imports; hash names
  │ 6. print: JS IR → text (+ source map), CSS rules → text
  ▼
Output { client, server, server_entry, manifest }
```

- **Step 1** reads only `k_sema` query results (types, resolutions, DI graph, route and protocol tables);
  codegen has no second checker. Repository queries are lowered to `k_db`'s IR here
  ([db.md §4](db.md#4-the-query-ir-and-lowering)); one that can't be translated, a migration that doesn't
  replay or migrations that don't create the entities' columns are `CodegenError::Unsupported` with the
  file, line and reason.
- **Step 2** is the split: the server roots are the `app`, every page, layout, endpoint page, protocol
  implementation, job and the stdlib server services the graph reaches; the client roots are every view
  page and layout, the `app`'s client entry and the client services they inject. A declaration is emitted
  into a bundle only if it is reachable from that bundle's roots *and* allowed on that side. **A project
  without an `app` is a library**: every top-level declaration of its own modules is a root on each side
  it is allowed on (this is what single-file codegen corpus cases are), and there is no server entry.
- **Step 3** is per declaration and parallel ([architecture.md §6](architecture.md#6-incremental-queries):
  `emit_module(module, target)`); its result is cached by the declaration's checked body.
- **The JS IR** is the only input of the printer: expressions, statements, functions, classes,
  imports/exports, template literals, plus a `Source(span)` annotation on nodes for source maps. Lowering
  never builds JS text by concatenation, so escaping is the printer's job in exactly one place: string
  literals are printed with `"` and JSON-compatible escapes, plus `\u2028`/`\u2029`, `<!--` and `</script`
  broken up (`<\u0021--`, `<\/script`) so output is safe to inline. Compiler-generated markup (template
  HTML, `out.raw` chunks, SQL as template literals) is printed with the same escapes except that comment
  markers stay: it holds no user text that wasn't escaped at build time.

---

## 4. Names and modules

- **One K module → one JS module per side.** `shop.cart` becomes `k:/app/shop/cart.js` on the server and
  `shop/cart.js` (then hashed) on the client. A module's file on one side contains only that side's
  declarations ([§16](#16-the-clientserver-split)), so a module with no client declarations has no client
  file.
- **Top-level declarations** are exported under their simple name. Imports between modules are named
  imports generated from resolutions (`import { CartService } from "./cart.js"`); K's one-declaration
  imports map one to one.
- **Reserved words.** A K identifier that is a JS reserved word or a global the output relies on
  (`delete`, `new`, `class`, `this`, `arguments`, `eval`, `undefined`, `NaN`, `Infinity`, `Object`,
  `Array`, `Map`, `Set`, `Symbol`, `Promise`, `JSON`, `Math`, `globalThis`, `$k`) gets a `$` suffix:
  `func delete()` is `delete$`. Members keep their names (property access is safe), except in object
  literal shorthands, which are written longhand.
- **Generated helpers** start with `$`: `$k` is the runtime namespace import
  (`import * as $k from "k:runtime"` — the bundler keeps only what is used), `$c` the cancellation context
  ([§8](#8-async-and-cancellation)), `$w_T` a witness ([§9](#9-generics-interfaces-and-witnesses)),
  `$ssr`-suffixed functions are server renderers.
- **Labels are erased.** Calls are positional in declaration order of the parameters; the checker already
  resolved labels and overloads. Overloads that share a name (`contains(_:)` and `contains(where:)`) are
  emitted with their label signature in the name: `contains`, `contains$where`.
- **Default arguments** are JS default parameters: an omitted argument in the middle of a call is passed
  as `undefined`, which K values never are. Defaults are evaluated in the callee, after the earlier
  parameters, as in the source.

---

## 5. Value representation

The table extends [architecture.md §7.1](architecture.md#71-value-representation), which stays the
summary.

| K | JavaScript | Notes |
|---|---|---|
| `Int` | `number`, always a safe integer | `+ - *` are wrapped in `$k.checkInt(…)` in development and test builds; release builds check only `/` and `%` (`$k.intDiv`, `$k.intRem`, which panic on zero). Integer `/` truncates toward zero. **Decision:** `number` on both sides (not `bigint`), so client and server compute identically; architecture §17 keeps the question open for later. |
| `Double` | `number` | Interpolation prints the shortest round-trip form with at least one decimal (`2.0`, `0.1`): `$k.formatDouble`. |
| `Decimal` | runtime `Decimal` (scaled `bigint` + scale) | `+ - *` and comparisons through methods; never a JS `number`. |
| `Money` | runtime `Money` (`Decimal` + currency code) | Mixed-currency `+`/`-`/`<` panic ([34-money.md](../design/34-money.md)). JSON `{"amount": "48.00", "currency": "USD"}`. |
| `Bool`, `String` | `boolean`, `string` | Strings are UTF-16 in JS; `count` on a `String` counts grapheme clusters through `Intl.Segmenter` (runtime helper). |
| `T?` | the value, or `null` for `nil` | `undefined` never represents `nil`. **`T??`** (an optional of an optional, only through generics such as `List<Int?>.first`) represents the inner `nil` as the frozen `$k.NONE` box so the two levels stay distinct. |
| `struct` | a plain object, never mutated after construction | Field order is declaration order (stable JSON and hidden classes). Assignment and passing copy the reference: value semantics come from never mutating ([§6.3](#63-mutation-and-copy-on-write)). |
| `enum` | `{ $: tag, …payload }`, tags in declaration order | Labelled payloads are fields by label (`{ $: 1, available: 3 }`); unlabelled ones `_0`, `_1`. Payload-free cases are frozen singletons created once per enum (`CartError.productNotFound`), so `===` compares them. Each enum module exports a constructor object: `CartError.outOfStock(3)`. |
| `class` | a JS class | Reference identity; `@State` tracks only reassignment ([08-types.md](../design/08-types.md)). |
| `service` | a JS class instantiated by the container ([§13](#13-services-and-dependency-injection)) | `@State` fields are signals, `@Derived` computed values. |
| `List<T>` | `Array` | Copy-on-write ([§6.3](#63-mutation-and-copy-on-write)). |
| `Map<K, V>`, `Set<T>` | `Map`/`Set` when the key's representation is a primitive or a payload-free enum; otherwise `$k.HashMap`/`$k.HashSet` keyed by structural hash and `equals` | JS `Map` compares objects by identity, which would break value semantics for struct keys. |
| `Duration` | milliseconds (`number`) | `200ms == 0.2.seconds` is `200 === 200`. |
| `Angle` | degrees (`number`); `Percent`: the number (`50%` → `50`) | |
| `Length`, `Color`, token categories | CSS text (`"16px"`, `"calc(100px + 2rem)"`, `"#3B82F6"`) | Folded at compile time ([§6.5](#65-units-and-calc-folding)). |
| `Date` | runtime `Instant` (epoch milliseconds + nanosecond remainder) | JSON RFC 3339. |
| `Uuid`, `Url`, `EmailAddress` | `string` (validated at construction) | Runtime text becomes one only through the runtime's parsers (`parseUuid`, `parseUrl`, `parseEmail`), which `T.parse` and the codecs share. **Decision:** a `Url` from runtime text is an absolute `http`, `https`, `ws` or `wss` URL with a host, never `javascript:` or another script scheme, so a parsed or decoded `Url` is always safe as a link target. |
| `Bytes` | `Uint8Array` | |
| functions, lambdas | functions, arrow functions | An `async` K function is an `async` JS function with the hidden `$c` parameter. |
| `Secret<T>` | `$k.Secret` wrapping the value | `toString`/`toJSON` give `Secret(•••)`; server-only. |
| `Void` | `undefined` as a return value only | Never stored. |

### 5.1 JSON (`Codable`)

Every `Codable` type gets a generated **codec pair** `T$encode(value) -> json` / `T$decode(json) -> value`
following [28-rpc-wire-format.md](../design/28-rpc-wire-format.md): field names are K names, payload-free
enum cases are their names, payload cases `{"case": "outOfStock", "available": 3}`, optionals are `null`,
`Date` RFC 3339, `Money` as in design 34. Decoding validates (types, required fields, enum names, unknown
fields are ignored) and returns a typed `DecodingError` instead of throwing a JS exception. Codecs are
used by RPC, `@Load` serialization, jobs, `Json`, local storage and the execution corpus.

- **Typed text values validate themselves** (15-forms-and-validation.md): `Uuid`, `Url`, `EmailAddress`
  and `Color` decode with `$k.codecs.uuid`, `url`, `email` and `color`, which accept only what the type's
  parser accepts, so `"javascript:alert(1)"` is a `mismatch` (an rpc's `invalid_argument`) and never a
  `Url`, and `"red; background: url(…)"` never a `Color` (a decoded color reaches `style` attributes as a
  custom property value). `Color.hex(text)` panics for text that isn't a hex color.
- **Maps**: a map whose key codec is text (`String`, `Uuid`, `Url`, `EmailAddress`, `Color`: codecs with
  `key: true`) is a JSON object, every other map a list of `[key, value]` pairs; each decodes only from
  its own shape (an array is never an empty map, a pair that isn't a two-element array is a `mismatch` at
  `path[i]`). Text-keyed maps encode into a prototype-less object, so a `__proto__` key is an entry.
- `Bytes` decode only from arrays of integers in `0...255`.

---

## 6. Expressions and statements

### 6.1 Operators

- Arithmetic as in §5; `String + String` is `+`; `List + List` is `[...a, ...b]`; `Bytes + Bytes` is
  `new Uint8Array([...a, ...b])`.
- `==`/`!=`: `===` for primitives and payload-free enums; `$k.equals(a, b)` (structural) for everything
  else that is `Equatable`; generated `T$equals` for types with custom equality (`Money`).
- `<`, `<=`, …: native for numbers and strings (strings compare by UTF-16 code units, matching
  `String`'s `Comparable` definition); runtime methods for `Date`, `Duration` (a number), `Money`,
  `Decimal`.
- `and`/`or`/`not` → `&&`/`||`/`!` (operands are always `Bool`).
- Optional chaining `a?.b` → `a?.b` with `?? null` when the result is stored (JS yields `undefined`).

### 6.2 Control flow

- `if`/`else` as an expression → a conditional expression when both arms are expressions without
  statements, otherwise an IIFE-free lowering into a temporary (`let $t; if (…) { $t = …; } else …`).
- `match` → `switch (v.$)` over tags for enums (with nested patterns lowered to `if` chains inside the
  case), `if` chains for literals and ranges; `where` guards fall through to the next arm. Exhaustiveness
  is checked, so the generated `default` is `$k.unreachable()` (a panic that can only fire on corrupt
  data decoded without validation, which codecs prevent).
- `guard let x = e else { … }` → `const x = e; if (x === null) { … }`.
- `for x in list` → `for (const x of list)`; `for i in 0..<n` → a counted loop; `for await` over a stream
  → `for await (const x of stream)` with cancellation ([§8](#8-async-and-cancellation)).
- The last expression of a lambda body is returned.

### 6.3 Mutation and copy-on-write

Structs and collections have value semantics without cloning on assignment:

- **Structs are never mutated in place.** `u.address.city = "Oslo"` on a `var u` compiles to a path copy:
  `u = { ...u, address: { ...u.address, city: "Oslo" } }`. A struct shares unchanged subtrees with its
  previous value, so a copy costs one object per level of the path.
- **Collections mutate in place when the `var` is uniquely owned.** The compiler tracks, per local
  `var`, whether its current value is *owned*: it is owned right after being assigned a fresh value (a
  literal, a copy, the result of a call that returns a new collection) and stops being owned when it is
  read as a value (assigned, passed, returned, captured by a lambda, stored in a field). A mutating method
  on an owned collection mutates (`xs.push(e)`); on an unowned one it first copies (`xs = xs.slice();
  xs.push(e)`) and the variable becomes owned. A loop appending to a fresh list therefore copies at most
  once. Fields of services and `@State` are never owned (they are observable), so their mutation is a
  copy plus one signal write.
- **`@State` paths.** On `@State` structs and collections the write goes through the path store
  ([runtime.md §3.3](runtime.md#33-path-stores)), which keeps the same copy semantics and notifies only
  readers of the changed path.

### 6.4 Strings

String interpolation is a template literal whose parts are formatted by type: `String` as is, `Int` by
`String(n)`, `Double` by `$k.formatDouble`, `Bool` by `String(b)`, optionals by the inner type or `nil`,
`Secret` as `Secret(•••)`, other types by their generated `T$$text` (two `$`, so a member named `text` can't collide with it). Static parts are printed with
template-literal escaping (`` ` ``, `\`, `${`).

### 6.5 Units and calc folding

`k_sema` has already folded constant unit expressions; codegen prints the folded literal. What remains:

- **Same-unit constants** fold to one literal: `16px * 2` → `"32px"`; durations to milliseconds.
- **Mixed lengths** become `calc()`: `100px + 2rem` → `"calc(100px + 2rem)"`; nested calcs are flattened
  (`calc(a + calc(b))` → `calc(a + b)`).
- **Dynamic operands** build the text at run time: `width + 2rem` with a `Length` variable →
  `` `calc(${width} + 2rem)` ``, flattened by `$k.calc` when `width` is itself a `calc(…)`.
- **Colors**: `.primary.opacity(0.8)` on a token → `"color-mix(in srgb, var(--k-color-primary) 80%,
  transparent)"`; on a literal, the computed hex (`#3B82F6CC`).

### 6.6 Collections and the stdlib

Stdlib methods with `@Native` bodies are implemented in the runtime ([runtime.md §12](runtime.md#12-native-stdlib-bodies))
or inlined when the JS equivalent has the same semantics: `filter`, `map`, `contains` on primitives,
`count`. `sortedBy` is a stable copy sort (`[...xs].sort(byKey)`, stable in ES2019+); `sum()` on
`List<Int>` folds with `checkInt` in development builds; `sum(in:_:)` uses `Money` arithmetic. Repository
collections are not arrays: their calls are SQL ([db.md](db.md)).

- **Builder structs** (`@Builder`, such as `HttpRequest`): a chained optional field is a copy with the
  field set, `HttpRequest(HttpMethod.get, url).query(q)` → `{ ...HttpRequest(HttpMethod.get, url), query: q }`.
- **Shorthand `if let name`** binds the visible local `name`, else the declaration's member `name`
  (`if let region` in a method of a struct with a `region` field reads `self.region`). A member a
  component holds in a signal (`@State`, `@Derived`) is read through it (`user.get()`), so a
  `@Derived` initializer with `if let user` tracks `user`.
- **Values of stdlib interfaces** (`Error`, `Codable`) are passed as they are: only project interfaces
  have adapter classes ([§9](#9-generics-interfaces-and-witnesses)).
- **`Router.navigate(url:)`** navigates to the URL's text; `Router.navigate(to:)` to `Page$href(page)`.

---

## 7. Errors, panics and typed throws

- **A typed error is the enum value itself, thrown with `throw`.** `try f()` is just `f()`; the marker is
  for readers and the checker.
- **Panics are `$k.KPanic` instances.** Every generated `catch` starts with
  `if ($k.isPanic(e)) throw e;`, so K code never catches a panic, then dispatches on the tag:

  ```js
  try {
    await this.cart.add($c, product);
  } catch (e) {
    if ($k.isPanic(e)) throw e;
    switch (e.$) {
      case 1: { const available = e.available; this.problem.set(CartError.outOfStock(available)); break; }
      case 3: { if (e._0.$ === 0 && e._0._0.$ === 0) { Router.navigate(Page.signIn()); break; } /* falls */ }
      default: this.problem.set(e);
    }
  }
  ```

- A `catch` that doesn't cover every case rethrows the rest (`default: throw e`) — the checker proved the
  function declares them.
- **Non-K exceptions** (a JS `TypeError` from a bug in the runtime or interop) are wrapped into a panic at
  the boundary that catches them (`$k.toPanic(e)`), never matched as typed errors.
- `try … else { fallback }` → a `try`/`catch` that returns the fallback for every non-panic error.
  `Result.of { … }` → the same, producing `{ $: 0, _0: value }` / `{ $: 1, _0: error }`.
- **Errors crossing the wire** are encoded by their codec; the RPC client decodes them back into enum
  values ([§15](#15-rpc-client-stubs-and-server-dispatch)).

---

## 8. Async and cancellation

- An `async` K function becomes an `async` JS function whose **first parameter is the cancellation
  context `$c`** (an `AbortSignal`, or `null` outside any cancellable run). Every call to an async function
  passes the caller's `$c`; effects, loads and handlers create one per run
  ([runtime.md §3.4](runtime.md#34-async-effects-and-cancellation)).
- **A cancelled `await` never resumes.** Every `await e` in K is printed as `await $k.guard($c, e)`, which
  returns a promise that settles like `e` while `$c` is not aborted and never settles after it is.
  Nothing after a cancelled await runs, so there is nothing to catch, as design 03 requires. (`guard` is
  elided for awaits whose promise is produced by a runtime primitive that already honours `$c`: `rpc`,
  `sleep`.)
- `rpc`, `clock.sleep`, `HttpClient` and database calls take `$c` and abort their I/O.
- Reads after the first `await` inside an effect are not tracked; the checker warns
  ([sema.md §12.3](sema.md#123-reactivity)), and the runtime simply doesn't see them.

---

## 9. Generics, interfaces and witnesses

- **Generics are erased**: `func first<T>(xs: List<T>) -> T?` is one JS function.
- **Structural constraints need no runtime help**: `Equatable`, `Hashable` and `Comparable` operations on
  a type parameter use the structural runtime helpers (`$k.equals`, `$k.hash`, `$k.compare`), which agree
  with the generated per-type ones.
- **Constraints whose operations depend on the type receive a witness.** For each type parameter with a
  `where T implements Codable` (or any interface with static requirements or generated code), the function
  takes a hidden parameter `$w_T` after `$c`: the type's codec pair, or its interface method table.
  `Json.encode<T>(value)` becomes `Json.encode($w_T, value)` and the call site passes `Order$codec`.
- **Interface method calls**: classes and services implement interfaces with real JS methods, so a call on
  an interface-typed value is a method call. **Structs and enums** keep their methods as module functions
  (`Point$distance(self, other)`, no prototypes, so copies stay plain objects); when such a value is
  converted to an interface type it is boxed as an existential: an instance of the generated adapter
  class `S$as$I` holding the value in `v`, whose methods (those of `I` and its parents, defaults
  included) call the module functions, so a call on an interface-typed value is a method call whatever
  the value. Conversion points are found by codegen from the recorded types (arguments, list elements,
  `let` annotations, fields).
- Extensions on a type compile to module functions in the extending module (`Int$retries(self)`); number
  extensions such as `10.minutes` fold at compile time when the receiver is constant.

---

## 10. Components on the client

A component compiles to a **factory** over the runtime ([architecture.md §7.2](architecture.md#72-components-client)
shows `Counter`):

```js
export function Counter(props, ctx) {
  const count = $k.signal(props.start());           // @State var count: Int = start
  const changed = props.on$changed ?? null;           // @Emits let changed: Event<Int>
  function increment() {                              // func increment()
    count.set($k.checkInt(count.get() + 1));
    changed?.(count.get());                           // changed.emit(count)
  }
  const root = t0();                                  // static DOM, cloned or claimed
  const label = root.firstChild.firstChild;
  $k.renderEffect(() => $k.text(label, `${props.label()}: ${count.get()}`));
  $k.on(root.lastChild, "click", increment);
  return root;
}
```

- **Props are getters** (`props.label()`), so a parent's signal flows into the child without re-running
  the child. Chained options are getters too; an option that wasn't set returns its default (evaluated
  once, in the child, like a K default).
- **`@Binding`** props are `{ get, set }` pairs; `bind user.address.city` creates a pair over the path
  store; `Binding(get:set:)` is the same pair from lambdas.
- **`@State`** → `signal` (or a path store for structs and collections), **`@Derived`** → `computed`,
  **`@Effect`** → `effect` (with `$c` when async), **`@Load`** → the load group of
  [runtime.md §7](runtime.md#7-load), **`@Form`** → `createForm`, **`@Inject`** → a lookup in `ctx.c`, the
  scope's container ([§13](#13-services-and-dependency-injection)).
- **`@Emits`**: `.on(changed: f)` on a component sets `props.on$changed`; `emit` calls it inside the
  child's batch.
- **`@Slot`**: an unscoped slot is a function returning nodes (`props.content()`), a scoped slot a function
  of its arguments (`props.row(item)`); optional slots are `null` when not given.
- **`@Stylable`**: a getter returning hashed class names from the parent's scope, appended to the
  element's class list.
- Each component instance runs inside `$k.scope`, so unmounting disposes its effects, loads and handlers
  ([runtime.md §3.5](runtime.md#35-owners-scopes-and-cleanup)).
- Handlers are wrapped by `$k.on`, which batches; async handlers get a `$c` that is aborted when the
  component unmounts. An async function of the component used as a value (`form.handle(buy)`, a
  closure) is `(...a) => buy($life, ...a)`, so it takes the arguments of its K function type alone.
- **A method value keeps its receiver**: `todos.clearCompleted` used as a handler is
  `todos.clearCompleted.bind(todos)` (an arrow over a receiver that isn't a plain path).
- **`@Load` and the members that read it.** On the server, `@Derived` values are computed after the
  awaited loads (they may read them); on the client, an `@Effect` of a declaration with loads returns
  while `$loads.values()` is `null` (it reads them, so it runs again once they arrive); `reload()` is the
  group's `$loads.reload()` anywhere in the declaration, closures and timers included, unless the
  declaration has a `func reload` of its own. The client decodes the hydrated values with their codecs
  (`(($v) => $v && { product: ProductSummary$codec.decode($v.product, "product") })($k.loadsOf(ctx))`),
  mirroring the server's `encode`. A declaration with a `failed` block passes a third argument, the error
  its server render failed with: `(($f) => $f == null ? undefined : LookupError$codec.decode($f,
  "failed"))($k.loadsOf(ctx, "failed"))` (the raw JSON for an error that isn't `Codable`), so hydration
  starts the group `FAILED` and the `failed` block claims the server's markup
  ([runtime.md §7](runtime.md#7-load)).
- **Titles in the browser.** A page's or layout's `metadata` block contributes its `title(_:)` as
  `$k.title(() => text)`, after the loads (`() => $loads.values() ? text : null` when it has loads, as the
  text usually reads them); a `metadata` block in a `failed` block contributes its title where the block
  renders. The other entries are server-only ([runtime.md §10](runtime.md#10-router)).
- **Service `init()`** (`MemberKind::Init`) becomes the class's `init` method, called at the end of the
  constructor, except for client-only services ([§13](#13-services-and-dependency-injection)).
- **`k.charts.LineChart`** is the runtime's (`charts.js`): a client chunk that uses it imports `LineChart`
  from the feature chunk `charts.[hash].js` (§17), and the server's renderer is
  `$s.natives["k.charts.LineChart$ssr"]` ([runtime.md §12.2](runtime.md#122-charts)). It takes its props as
  getters like any component.
- **The stdlib's `@Native` components** have no generated body; the view lowers them in place:
  `Stack` is a flex `div` (`axis` and a `gap` token such as `.md`, read at compile time), `Grid` a grid
  `div` with an integer `columns` literal (default 2), `Spacer` a `flex: 1 1 auto` `div` hidden from
  assistive technology, `VisuallyHidden` a `span` with the visually-hidden styles, `Icon` a `span` with
  `data-icon` (decorative, or `role="img"` with `aria-label` when it has a `label`). Their layout values
  are inline `style` text; an argument that isn't a literal keeps the default (*v0 limitation*).
  **`ErrorBoundary`** is a block: on the client `branch(start, end, failed ? 1 : 0, [content, fallback])`,
  where the content arm renders inside `catchError` and a `try`, so a panic while it renders, or later in
  one of its effects, shows the fallback (an empty range without one); on the server a `try` around the
  content that takes back what the content wrote (`out.chunks.length`) and writes the fallback.

---

## 11. Views: DOM creation, SSR emission, hydration markers

### 11.1 Static extraction

The view tree is split into **static HTML** and **dynamic parts**. Each maximal static subtree with one
root element becomes one template string (`$k.template("<div><span> </span><button>Add</button></div>")`),
created once per module. Elements map to their HTML tags through the element table
([sema.md §5.4](sema.md#54-data-tables)): `box` → `div`, `paragraph` → `p`, `link` → `a` with a generated
`href`, `image` → `img` with `alt` from `description`.

Dynamic parts and how they are reached in the cloned tree:

| Part | In the template | Binding |
|---|---|---|
| dynamic text | a single space text node | `renderEffect(() => text(node, …))` |
| two adjacent dynamic texts | `<!---->` between them | one effect each |
| dynamic attribute | omitted | `renderEffect(() => attr(el, "aria-pressed", …))` |
| `.class([.a, .b when x])` | static classes in `class` | `classList(el, "a_h", [x() && "b_h"])` |
| reactive style value `{accent}` | the class's CSS uses `var(--k-…)` | `style(el, "--k-…", …)` |
| `.on(event: f)` | — | `on(el, "event", f)` |
| `if`, `if let` | `<!--[--><!--]-->` | `when(start, end, cond, render, otherwise)`; an `if let x` binding is the getter `x$()`, which reads the value again (the last non-nil one), so the branch follows a new non-nil value without rendering again; an optional slot keeps a plain binding |
| `match` | `<!--[--><!--]-->` | `branch(start, end, select, arms)` |
| `for … key` | `<!--[--><!--]-->` | `each(start, end, items, key, row)` |
| component | `<!--[--><!--]-->` | the child factory's nodes inserted between the markers |
| slot | `<!--[--><!--]-->` | the slot function's nodes |

Nodes are reached by **paths** (`root.firstChild.nextSibling.firstChild`), computed at compile time and
printed as property chains; a path longer than four steps gets a cached intermediate.

- **Adjacent texts.** HTML parsing merges adjacent texts into one node, so a run of texts that holds a
  static one (`paragraph { "Total: " total }`, `"a" "b"`) is lowered to **one** text node whose binding
  writes the joined text (`VNode::Mixed`), and an empty static text is dropped; only a run of dynamic
  texts keeps one node each, with `<!---->` between them (in templates, in fragments of a view's or a
  branch's roots, and in SSR alike).
- **Template constants** are module-level and named after their declaration (`Header$t0`), because a
  chunk concatenates many declarations' preludes.
- **Keyed rows** read their item through the item signal at each use (`entry$()`), so every binding of a
  row tracks its item and a new item under the same key re-renders the row in place.
- **Labeled controls** (`.label("Quantity")`): the `<label>` precedes the control. At the root of a view
  it is its own template created before the control's; nested in a template it is part of it (a `Label`
  part), and both get one `uid()` (`for` and `id`), in document order like the server's `out.uid()`.
- **Control values.** `bind place` is two-way through the place (a path store's binding pair for
  `@State` structs); a `@Form` field (`bind order.quantity`) is wired by the form,
  `order.field("quantity").attach(input)`, which syncs the value and shows the field's error under the
  input (runtime.md §9); a `Binding` value (`isChecked: Binding(get:set:)`) is read with `get()` and
  written with `set(_:)`; a plain `isChecked:` value sets `checked` one way.
- **`match` guards** (`case .loaded(let places) where places.isEmpty`) are part of the arm's test, with
  the arm's bindings read straight from the scrutinee.

### 11.2 SSR emission

The same component also compiles to a **server renderer** `Counter$ssr(props, ctx, out)` that appends to
an `Out` buffer ([runtime.md §11](runtime.md#11-ssr)). It has no signals, no DOM and no effects: `@State`
initial values are plain locals, `@Derived` values are computed once, `@Load` values were awaited before
rendering, effects and `init()` of client-only services don't run.

```js
export function Counter$ssr(props, ctx, out) {
  const count = props.start();
  out.raw("<div><span>");
  out.text(`${props.label()}: ${count}`);
  out.raw("</span><button>Add</button></div>");
}
```

- Static parts are emitted as raw strings that the compiler escaped at build time; **every dynamic value
  goes through `out.text` or `out.attribute`**, which escape ([runtime.md §11](runtime.md#11-ssr)). There
  is no raw-HTML construct in K, so no other path exists.
- Blocks emit `OPEN` and `CLOSE` markers around their content; adjacent dynamic texts emit `SEPARATOR`;
  dynamic attributes are emitted inline with the static ones. A declaration with `@Load` frames the view
  (or its `failed` block) it rendered with `OPEN`/`CLOSE`, the range of the client's branch over the
  load group's state.
- Each component instance has a **hydration key**, its path in the instance tree (`0.2.1`), under which its
  serialized state (load results, form values) is stored in `k-data`.
- Label ids come from `out.uid()`, a counter per request shared by a page's and its layouts' buffers
  (never an isolate-global one), so the same page renders the same HTML and the hydrating client, which
  restarts `uid()` at `k1`, gets the same ids.

### 11.3 Hydration

On the client, `template()` *claims* the next server-rendered node instead of cloning, text bindings
attach to the existing text nodes, and blocks find their markers. The roots of a view or a branch that
aren't elements are claimed too: `$k.textNode(text)` (a text or dynamic text, evaluated in order with the
other roots' claims), `$k.separator()` (the `<!---->` between two dynamic texts) and `$k.markers()` (a
block's marker pair, with its content) take the server's node while hydrating and create one otherwise. The protocol, the data format and
mismatch handling are in [runtime.md §6](runtime.md#6-hydration-protocol). Codegen guarantees that the
client factory and the server renderer walk the same structure in the same order: both are printed from
one lowered view, so they can't drift.

---

## 12. Styles, themes and variants

- **Scoped class names.** Class `panel` of component `shop.CartSummary` is `panel_<suffix>` where the
  suffix is `hash::scope_suffix("shop.CartSummary")`: six base-36 digits of the hash of the qualified
  name, stable across style edits and deploys. Two components whose suffixes collide in one build are
  detected and the later one (by qualified name) gets the suffix of `name + "#1"`; the case is a unit test.
  Keyframes are scoped the same way. `global("btn-primary")` is the name as written.
- **Rules.** Each style block becomes rules in source order. Nesting is flattened (`.summary { &:hover {…}
  }` → `.summary_x:hover {…}`), `@media`/`@container`/`@layer` wrap the flattened rules, property names
  are converted to kebab case (`borderLeft` → `border-left`).
- **Values.** Implicit members resolve to tokens and print as custom properties (`padding: .md` →
  `padding: var(--k-space-md)`); literals print as written; units fold as in §6.5.
- **Readable names.** Under `Profile::Test` the scope suffix is replaced by the component's simple name:
  classes are `Component__class`, keyframes `Component__name`, reactive properties `--k-Component-<n>`.
- **Reactive values.** `{accent}` in a rule becomes `var(--k-<suffix>-<n>)` (`n` counts the block's
  interpolations from 0), and the elements that carry
  the class get a `style(el, "--k-<suffix>-<n>", value)` binding; on the server the property is emitted in
  the element's `style` attribute.
- **Theme tokens** become custom properties on `:root` in declaration order: `--k-color-primary:
  #3B82F6;`, named `--k-<category>-<token>` with both parts in kebab case (`color`, `space`, `radius`,
  `font-size`, `font`; `surfaceHover` → `--k-color-surface-hover`). A `font` token's
  `Font(family: "Inter", size: 16px)` prints as the value of the `font` shorthand, size then family:
  `--k-font-body: 16px "Inter";` (the K string literal is the CSS string), in `:root` and in variants
  alike, so `font: .body` is valid CSS; every other value prints as written.
- **Variants** become classes on `<html>`: `:root.k-theme-dark { --k-color-surface: #111827; }`. A variant
  with `matches .prefersDark` also gets `@media (prefers-color-scheme: dark) { :root:not(.k-theme-set) {…}
  }`, so the OS preference applies until `theme.apply` adds `k-theme-set` plus the chosen variant's class.
  SSR reads the `k_theme` cookie and renders both classes on `<html>` from the first byte.
- **Printing.** Development and test output put one declaration per line, indented by two spaces, a
  rule's selector and `{` on one line, `}` on its own; rules in source order after the `:root` block.
- **Output.** One stylesheet, `app.[hash].css`, with the theme and every component's rules, linked from
  the HTML shell (per-page stylesheets are *deferred*). Release builds strip whitespace and comments.

---

## 13. Services and dependency injection

The DI graph is resolved at compile time per environment ([10-services.md](../design/10-services.md)); the
generated code is plain construction.

- **A service compiles to a class.** Its constructor receives the resolved dependencies in `@Inject`
  declaration order, runs field initializers in source order, then `init()`:

  ```js
  export class CartService {
    constructor($deps) {
      this.api = $deps.CartProtocol;                    // @Inject let api: CartProtocol
      this.items = $k.signal([]);                       // @State var items
      this.total = $k.computed(() => /* sum(in:) */);   // @Derived let total
    }
  }
  ```

- **On the server (implemented)** the entry `k:/app/$server.js` emits two containers: `$Process` (one per
  isolate: `@Scope(.server)` services, wrapped by `$s.shareState(instance, "module.Name", [@State
  fields])` so their state is the process's across workers, and configs, `C$load()` once: each `@Env`
  field read with `$s.config(name)`, `$s.configEnum(name, cases)`, or `$s.configSecret(name)` for a
  `Secret<T>` or `Secret<T>?`, which is `null` when unset; a stdlib config the app injects itself, such as
  `k.rpc.ServiceConfig`, is built the same way from the stdlib's `@Env` names and joins the manifest's
  configuration, except in a test build, which never reads the environment and gets the panicking
  `$s.missingNative` stand-in) and
  `$Request` (`constructor(st)`, one per request: every other service, memoized getters
  `get X() { return this.$X ??= new X({ Dep: this.Dep }); }`, `.server` getters delegating to
  `$process`, `Session`/`SessionManager` from the request state, `Database` (`$s.database()`, one per
  request, so its transaction binds the request's repository calls), `Clock` (`$s.clock()`), every
  repository (`new R({ Database: this.Database, …injected })`), and for each protocol with an
  implementation an in-process client `P$local` whose methods go through the rpc table,
  `m($c, ...a) { return $s.callLocal(this.c, $spec.rpcs["/pkg.P/M"], $c, a); }`, so the rpc's
  validation and guards run before the implementation in the same container
  ([§15](#15-rpc-client-stubs-and-server-dispatch))). Pages, layouts and components read `ctx.c.X`;
  endpoint handlers `$e.c.X`.
  Every other injected type gets a getter too: an **interface** delegates to its implementation's
  getter, chosen by the app's first `use … for … in` rule that names the selected environment (a rule
  without `in` applies to all), else the single non-test project service implementing it;
  else the stdlib's implementation ([§13.2](#132-platform-services-on-the-server));
  **`k.server.PublicUrls`** is `$s.publicUrls(Page$href)` (the public origin plus the page's path;
  `absolute(endpoint:)` panics in v0); anything else, a stdlib service the v0 server doesn't provide,
  is `$s.missingNative(name)`, so injecting it works and calling it panics with its name
  (`tests/e2e/interface-bindings`).
- **In the browser (implemented)** the client entry (`app.[hash].js`, `app::client_containers`) emits
  `$App`, one per app, and, only when the app has `@Scope(.route)` services, `$Route extends $App`, one
  per mounted page. The router calls `options.c()` for every page it mounts and passes the result as
  `ctx.c` (hydration included), so generated code reads `ctx.c.X` exactly as on the server:

  ```js
  import { CartService, CartProtocol$client, StorageReadError, StorageWriteError } from "./shared.[hash].js";
  class $App {
    constructor(d) { this.$a = this; this.$d = d; }            // d: the k-data payload, null when client-rendered
    get CartService() { return this.$a.$CartService ??= new CartService({ CartProtocol: this.$a.CartProtocol }); }
    get CartProtocol() { return this.$a.$CartProtocol ??= new CartProtocol$client(); }
    get LocalStorage() { return this.$a.BrowserStorage; }       // the stdlib default of the interface
    get BrowserStorage() { return this.$a.$BrowserStorage ??= $k.browserStorage(StorageReadError, StorageWriteError); }
    get Clock() { return this.$a.$Clock ??= $k.clock(this.$a.$d); }
    get Draft() { return new Draft({ Serials: this.Serials }); } // @Scope(.component): new on every read
  }
  class $Route extends $App {                                    // only with @Scope(.route) services
    constructor(a) { super(a.$d); this.$a = a; }
    get Visit() { return this.$Visit ??= new Visit({ Serials: this.Serials }); }
  }
  const data = $k.readData(document);                            // `null` under @Rendering(.client)
  const app = new $App(data);
  $k.startRouter(routes, container, { c: () => new $Route(app), data });  // `c: () => app` without $Route
  ```

  - **Scopes.** Every `$App` getter memoizes on `this.$a`, the app container, and resolves its
    dependencies there, so an app service read through a `$Route` (or a provided container) is still the
    app's one instance; a route service is memoized on its `$Route` and lives while its page is mounted;
    a component-scoped getter constructs on every read, and **the component that injects it provides that
    instance to its subtree**: `const draft = ctx.c.Draft; ctx = $k.provide(ctx, "Draft", draft);`
    (`provide` wraps the container in a proxy answering that one name). The server's `$Request` does
    the same for component-scoped services (an unmemoized getter plus `provide` in the SSR renderer).
    Tested by `the_browser_container_scopes_services_per_app_route_and_component`
    (`crates/k_server/tests/client_bundle.rs`).
  - **What is bound.** Only what the client roots reach (`app::client_reach`): starting from the
    `app`, its pages and layouts, the declarations their client items use, every `@Inject` dependency,
    and for an injected interface the implementation bound for the selected environment. A service only
    server code injects (`examples/shop`'s `PayboxCheckout`, the `PaymentGateway` of production and
    development, which only `StoreApi` uses) or one nothing injects has no getter and is in no client file, even though
    nothing marks it server-only (`codegen/split/client-holds-only-reachable-services`). Within that
    set: every client service of the project; every protocol to its generated RPC client
    `P$client` ([§15](#15-rpc-client-stubs-and-server-dispatch)); every interface that client code
    injects to the app's `use … for … in` rule for the selected environment, else its single project
    implementation, else the stdlib default (`LocalStorage` → `BrowserStorage`, `HttpClient` →
    `FetchHttpClient`); the stdlib's services to their browser bodies in `services.js`
    ([runtime.md §12](runtime.md#121-services-in-the-browser)): `Clock` → `$k.clock(data)`, `Theme` →
    `$k.theme()`, `Session` → `$k.session(data)`, `BrowserStorage` → `$k.browserStorage(R, W)`,
    `FetchHttpClient` → `$k.fetchHttpClient(HttpError, HttpStatus)` (the generated stdlib enums whose
    cases they throw or return are passed in, so `catch .unreadable` compares the same singletons).
    Anything else is `$k.native(name)`, which panics with its name when used.
  - **Test-only modules** (`testing` directories, `DeclInfo::in_test_module`) are left out of both bundles
    and of interface bindings outside `Environment::Test`, so a `testing/FakeHttpClient` never ships and
    never becomes the "single implementation" of `HttpClient`.
  - **Client-only `init()`.** A client-only service's constructor runs `init()` through
    `$k.afterHydration(() => this.init())`: after the hydration pass when one is running (the first
    paint matches the server's), at once otherwise; the server renderer never calls it. Other services
    call `this.init()` at the end of the constructor on both sides.
- **Configs** are values built at startup from the environment ([server.md §8](server.md#8-configuration-and-secrets));
  the process container holds them. They exist only on the server.
- **Client-only services during SSR** exist with their declared initial state and their `init()` is not
  called. `Theme` is `$s.theme(st)` there: `current` is the variant the `k_theme` cookie remembers, or
  `base`, and `apply` does nothing.
- **Tests** (`Environment::Test`) build a fresh container per test with the test group's `use` rules.

### 13.1 Repositories and entities (implemented)

- **A repository compiles to a class** (`db.rs`): `this.$db = $deps.Database`, its `@Inject` fields, its
  functions lowered as service methods, and the inherited writes `$insert`, `$insertAll`, `$update`,
  `$delete` ([db.md §7](db.md#7-writes-version-and-unique-violations)).
- **An awaited query** on the repository's collection is one call: `$s.dbQuery(this.$db, kind, n,
  $s.dbParams([[codec, value], …]), R$E$dbRow, [relations], "E")`, `$s.dbCount`, or `$s.dbWrite` for bulk
  updates and deletes, where `n` is the statement's number in the database manifest. No SQL, table or
  column name appears in JavaScript (tested). An awaited write on a repository value (`await
  accounts.insert(a)`) calls `$insert($c, a)`.
- **Per entity and repository** the module prelude gets `R$E$dbCols()` (the column codecs, built on first
  use), `R$E$dbRow(r)` (a row to the entity, through its constructor) and `R$E$dbValues(e, out)` (the
  entity's column values).
- **`catch .field`** after a `try` of a repository write tests the `UniqueViolation` case's tag (the
  field's position among the entity's `@Unique` fields), since sema types `Entity.UniqueViolation` as an
  error type.

### 13.2 Platform services on the server

`server::bound_services` and `server::stdlib_default` (implemented; [server.md §18](server.md#18-outgoing-http-and-email)):

| Injected | Production and development | `Environment::Test` |
|---|---|---|
| `k.http.HttpClient` (no project binding) | `FetchHttpClient` → `$s.fetchHttpClient(HttpError, HttpStatus, [codes])` | `TestHttpClient` → `$s.testHttpClient(…)` |
| `k.server.EmailProvider` | `SmtpEmailProvider` → `$s.smtpEmailProvider(EmailError, $s.info().config)` | `RecordingEmailProvider` → `$s.recordingEmailProvider(EmailError)` |
| `k.server.JobQueue` | `DatabaseJobQueue` → `$s.jobQueue(this.Database)` | `RecordingJobQueue` → `$s.recordingJobQueue()` |
| `k.test.InlineJobQueue` (a `use` rule) | `$s.inlineJobQueue(this.$r.spec, this, this.Database)` | the same |

- The interface's getter delegates to the implementation's (`get HttpClient() { return
  this.FetchHttpClient; }`), so an app that also injects the implementation directly gets the same
  instance; an implementation's getter memoizes per request container.
- `[codes]` is `HttpStatus`'s codes in declaration order (`null` for `other`), from which the runtime
  maps received statuses to cases; the same order is `HTTP_CODES` in `std.js` (a codegen test keeps
  them equal), which `status.code` (`$k.httpCode`) and `isSuccess`/`isClientError`/`isServerError`
  (inline comparisons of it) read.
- The stdlib enums these receive (`HttpError`, `HttpStatus`, `EmailError`) are roots of the server
  bundle's stdlib pass, so they are generated even when no project code names them.
- **Configuration**: `SmtpConfig` is added to the manifest only when a reachable declaration injects
  `SmtpEmailProvider`, or `EmailProvider` bound to it; the stdlib's enum-typed config fields carry their
  cases (`SMTP_SECURITY`: `startTls`, `tls`, `none`).
- **Property or method**: a read of a member name that is both a property and a method of a type
  (`HttpResponse.body`, the received text, and `body(_:)`, the builder step) is the property; only a
  call is the method.

### 13.3 Background jobs

`jobs.rs` (implemented; [server.md §11](server.md#11-background-jobs)). For every non-test service
implementing `k.server.Job<Payload>`:

- **Name**: `@JobName("…")`; else the `[jobs.<name>]` table of `storage.lock` whose `declaration` is the
  service's qualified name (so a renamed service keeps its queued rows); else the service name in kebab
  case.
- **`kind`**: after the class, `Job.kind = Object.freeze({ name: "…" });` (`JobQueue.enqueue` reads
  `kind.name`; the payload's codec is passed before it, as for any `Codable` generic).
- **Spec table**: the server spec's `jobs` maps each name to `{ run: (c, $c, j) =>
  c.Job.run($c, Payload$codec.decode(j, "payload")) }` (`Void` payloads pass nothing).
- **Manifest**: `Manifest::jobs`, a `k_db::stdlib::JobSpec` per job: name, declaration, `@Retry(attempts:
  backoff:)` (default 5 attempts, 10 s; durations such as `30.seconds`, `500.milliseconds`) and
  `@Schedule` (`.every(1.hour)`, `.daily(at: TimeOfDay("03:00"))`, `.weekly(on: .monday, at: …)`).
- `JobConfig` (`K_JOB_CONCURRENCY`) is in the manifest when the program has a job.

---

## 14. Routing tables

- The generated **`Page` enum** is an ordinary enum ([§5](#5-value-representation)): a page without
  parameters is a unit case, a page with parameters a function of all of them in declaration order
  (optional ones defaulted), so `.search` used as a value is `Page.search()`; a chained query parameter
  (`.product(id: 1).tab(.reviews)`) is a copy `{ ...value, tab }`; `Page$href` builds the URL (required
  parameters as path segments, optional ones in the query string omitted at their default; a `Page`
  parameter's text is its URL) and `page.url` is `Page$href(page)`. URL templates: `at "…"` (its source
  text, `{param}` included), else the name in kebab case (`Home` is `/home`) with required parameters
  appended.
- **Parsing** (server): each route's `match(url)` is `$s.route(url, segments, [[name, parser, required,
  default]])`; parsers by type (`$s.param.int`, `double`, `bool`, `uuid`, `string`, `cases({…})` for a
  payload-free enum by exact case name, `page(() => $spec)` for a `Page`: a path of this app only), so a
  failed parse is a non-match and a bad `id` is a 404. **The client route table uses the same matcher**:
  `parse: (u) => $k.route(u, segments, params)` with `$k.routeParam.int` etc. (`route` and the parsers
  live in `std.js`, shared by both sides), plus `value: (p) => Page.case(…)` for `Page` parameters
  (`routeParam.page(() => routes)`), so a client-rendered page gets an `Int` where the server gets one, a
  URL the server answers with 404 matches no client route, and a query parameter never overrides a path
  parameter.
- **The route table** is one array of `{ page, segments, load }` sorted by specificity (static segments
  before parameters, longer before shorter, then by name), shared by the client router and the server, so
  both resolve every URL identically. `load` imports the page's chunk on the client and is a direct
  reference on the server. On the client it resolves to `{ layouts, render }`: `layouts` the page's
  layout components, outermost first (`[Storefront]`), and `render(params, ctx)` the page alone
  (`m.Home($k.paramProps(params), { ...ctx, key: "0" })`). The router renders the layouts around it with
  the keys the server used (`"L{i}"`), so a server-rendered document hydrates as a whole, and keeps the
  layouts the next page shares mounted ([runtime.md §10](runtime.md#10-router)).
- **Redirects** recorded in `routes.lock` for renamed pages become `308 Permanent Redirect` entries on the
  server (implemented: `redirect "/old/{id}" -> page` lines; the old template is parsed with the new
  page's parameters, so the redirect keeps them and the query) and `history.replaceState` on the client
  (planned).
- **Endpoint pages** are in a separate server-only table with their methods (`Endpoint` enum), never in
  the client bundle. Each function of the page is a module function `Page$fn($e, …)` taking the
  request's `$e` (its `@Param`s and container) first; a handler calling the page's own function calls it
  with the same `$e` (`tests/e2e/endpoints`, `/helper`).
- Guards (`requires`) are compiled into the table entry as `["auth"]`, `["role", Role.case]` and
  `["rate", key, count, windowMs]` (from the literal `N.perMinute`): the server evaluates them for every
  request, the client only to decide navigation ([runtime.md §10](runtime.md#10-router)).
- **The spec.** The server entry passes `{ pages, redirects, rpcs, endpoints, container, appMeta,
  signIn, session: {roles, decodeUser}, httpStatus, httpBody, assets, rendering }` to `$s.createApp`
  (documented on `createApp` in `app.js`); `assets` is filled with the hashed client file names after
  the client chunks are built. Layouts come from the page's or the app's `@Layout(…)` (none with
  `@NoLayout`); `signIn` from `@SignInPage(.case)` with the refused page as `returnTo`.

---

## 15. RPC: client stubs and server dispatch

Wire format: [28-rpc-wire-format.md](../design/28-rpc-wire-format.md) and
[server.md §6](server.md#6-rpc-dispatch).

- **Client stub.** For each protocol used by client code, a class implementing it:

  ```js
  export class CartProtocol$client {
    async add($c, request) {
      const o = await $k.rpc("/shop.cart.CartProtocol/Add", { request: AddItemRequest$encode(request) }, $c);
      if (o.ok) return CartItem$decode(o.message);
      throw CartError$fromRpc(o);       // typed case, or rpcFailure(.offline/.timeout/.rejected/.invalid)
    }
  }
  ```

  `E$fromRpc` maps an outcome to the rpc's error enum (generated as `$k.rpcError(o, E, E$codec)`, the codec
  only for a `Codable` error): `failed_precondition` with a `k.TypedError` detail → the named case decoded
  by the error's codec from `{case, ...fields}` (without a codec, the case called with the field values in
  order); `invalid_argument` → `rpcFailure(.invalid(…))`;
  `unauthenticated`/`permission_denied`/`resource_exhausted` → `rpcFailure(.rejected(…))`; `offline` and
  `timeout` → the network cases; `internal`, `unimplemented`, an undecodable body → a panic (a server bug,
  not a domain outcome).
- **Server dispatch.** One table `{ path → { guards, decode, validate, call, encode, error, failure, stream } }` in
  the server entry: `decode(j)` reads each parameter's internal name with its codec, `async validate(a, c,
  $c)` calls a generated `async X$check(value, path, failures, c, $c)` for every `Validatable` value in a
  parameter (the parameter itself, inside optionals, and each element of a list or set under
  `requests[1]`; nested `Validatable` fields likewise): each field's constraint annotations (`@Length`
  counting characters as `String.count`, `@Range`, `@NotBlank`, `@Required`/`@NotEmpty`, `@Email` as
  `EmailAddress.parse`, `@Matches` with `new RegExp(pattern)`, and `@Check(Service.f)`, which awaits
  `c.Service.f($c, value)` in the request container), the first failure per field, then `X$validate` with
  a `rules` collecting `{path, message}`, `call(c, $c, a)` the implementation in the
  request container, `encode` the result (a struct is the message; anything else `{value}`; none `{}`),
  `error` `{ name: "package.module.Error", text: E$text, fields }` for the declared error (`fields` is
  `(e) => $k.payloadFields(e, E$codec.encode)` for a `Codable` error, the codec's form without `case`,
  which the client stub decodes with the same codec; else `$k.payloadFields`, the structural JSON), `failure`
  `(f) => E.rpcFailure(f)` (an `RpcFailure` as the rpc's own error, for in-process calls; `null` without
  an error enum). The client stub
  sends parameters under their internal names and reads the result the same way.
- **In-process calls during SSR** use the same table minus HTTP: the generated `P$local` client calls
  `$s.callLocal(c, entry, $c, args)`, which runs the entry's validation (no decoding: the arguments are
  already values) and guards against the current request, then `call` in the same container; a
  failure is `entry.failure(rpcFailure)` (implemented, `tests/e2e/in-process-rpc`;
  [server.md §6](server.md#6-rpc-dispatch)).
- `Stream<T>` server streaming: `stream: true` and an element encoder; the runtime sends Connect
  envelopes ([server.md §6](server.md#6-rpc-dispatch)). A `Broadcast` field of a `@Scope(.server)` service is
  a `new $s.ServerBroadcast(name, T$codec)`, so a publish on any worker reaches every subscriber, and the
  element crosses isolates in its codec's encoding and comes out decoded, with its types (before, it
  crossed as structural JSON, and an element with a `Date` or a `Uuid` field broke the stream).
- **Remote clients** ([40-workspaces-and-services.md](../design/40-workspaces-and-services.md)). The
  server emits `P$remote` for every protocol the app calls in another service and doesn't implement
  (`Gen::remote_protocols`: the targets of `use remote(…) for P` rules and the result types of
  `RemoteClient.at` calls). It is the client stub with a constructor taking a remote binding `r` and
  `$s.remoteCall(this.r, path, message, $c)` in place of `$k.rpc(…)`; decoding and `$k.rpcError` are
  the same, so a paybox error arrives in shop typed. The request container binds `P` to
  `new P$remote($s.remoteOf("<app>", this.<Config>))` for the environment's `remote` rule (its config
  counts as reached, so the manifest lists its variables), or to `new P$remote($s.testRemote("<app>"))`
  in a test build. `RemoteClient.at(url:token:timeout:)` lowers to
  `new P$remote($s.remote("<app>", url, token, timeout))`, `P` the call's type.
- **Rpc paths of library protocols.** The Protobuf package is the module prefixed with the name of the
  package that **declares** the protocol (`Gen::package_name`): for a declaration mounted from a
  workspace library, the library's `[package] name` read from its mounted `k.toml`. So the serving and the
  calling app both use `/pay_contracts.pay.Payments/Charge`.
- **`ServiceAuthenticated`** in a `requires` clause is the guard `["service", () =>
  $s.configSecret("K_SERVICE_TOKEN")]`; the stdlib's `ServiceConfig` joins the manifest's configuration
  when a served protocol requires it. `ServiceCaller` is `$s.serviceCaller(this.$r)`.
- **The manifest** lists `remotes` (each remote rule's protocol and its config's `url` and `token`
  variables) and `serves` (the protocols the app implements), which `k dev` at a workspace root wires.
- A test's fake (a service in a test file or test-only module) is never a protocol's implementation:
  it is bound by the test's `use` rule.
- **The client stub of a streaming rpc** passes the element decoder to the runtime's `streamed` and
  returns the open stream:

  ```js
  async watch($c) {
    const o = await $k.rpc("/notes.NotesProtocol/Watch", {}, $c, streamed((m) => NoteChange$codec.decode(m.value, "value")));
    if (o.ok) return o.message;
    throw $k.rpcError(o, RpcOnlyError);
  }
  ```

  `streamed` is a symbol of `k.core` that the feature chunk `streams.[hash].js` implements (§17), imported
  by the chunk that holds the stub ([runtime.md §8](runtime.md#8-rpc-client)).

---

## 16. The client/server split

**Guarantee: no code or data of a server-only declaration is ever in a client file.** It holds by
construction at three levels, and a fourth level checks it:

1. **Language** (`k_sema`): client code can't reference a server-only declaration (K-level boundary
   errors with the reference path, [sema.md §12.6](sema.md#126-services-di-layering-serverclient-boundary)).
   Secrets and configs are server-only types, and `Secret` isn't `Codable`, so no value of one can be
   sent to a client.
2. **Reachability** (step 2 of the pipeline): the client bundle is generated only from the client roots;
   a declaration is lowered for the client only if it is reachable from them *and* its side is not
   server-only. Modules are split per declaration, so a shared module's client file omits its
   server-only neighbours, and their string literals with them. For an app the roots are its pages,
   layouts and the client entry, and reachability follows symbol uses and `@Inject` edges, so a helper
   function only server code calls stays out of the client (`projects/split`).
3. **Values**: configuration and secrets don't exist at build time at all; they are read from the
   environment at server startup. Generated SQL, rpc handler code and `k:runtime/server` exist only in
   server modules.
4. **Verification**: `check_client_bundle` scans every client file for the server runtime
   (`k:runtime/server`, `Deno.core`, `op_k_`), server module specifiers (`k:/app/`) and caller-supplied
   strings (names of server-only declarations, secret values). The codegen corpus runs it on every case,
   `.expect` files assert `client lacks …` for named declarations and literals, and the e2e suite asserts
   that served client assets lack the test secrets.

The serialized hydration data (`k-data`) contains only `Codable` values the page's own loads returned and
the `Session` mirror (`user`), which are by definition meant for the browser.

---

## 17. Bundling, chunks and printing

- **Chunks**: `runtime.[hash].js` (the embedded runtime's client files as one module, compacted in
  release builds), `shared.[hash].js` (every client declaration that is not a page, hoisted into one
  module: generated names are unique across the program), one `pages/<name>.[hash].js` per page of an
  app, importing from `shared`, and `app.[hash].js` (the route table and the client entry, loading pages
  with `import()`). **Decision:** shared code is its own chunk rather than part of `app`, so the import
  graph `runtime ← shared ← pages ← app` has no cycle and every hash can cover the names it imports.
  Splitting modules used by one page into that page's chunk is *deferred*.
- **Imports** between chunks use relative URLs with hashed names; the hash of a chunk covers its contents
  *and* the names it imports, so changing a dependency renames its dependents. The entry imports the
  services, RPC clients, layouts and stdlib enums its containers and route table name from `shared`.
- **Feature chunks**: `charts.[hash].js`, `drag.[hash].js`, `files.[hash].js`, `keys.[hash].js`,
  `http.[hash].js` and `streams.[hash].js` (`runtime/charts.js`, …), emitted only when a client chunk or
  the client entry uses a symbol they implement (`k.charts.LineChart`; `k.drag.ReorderableList`,
  `Draggable`, `DropTarget`, `DragKind(name:)`; `k.ui.KeyboardShortcut`; `streamed`; and
  `k.device.BrowserFiles`, which the entry's `$App` constructs for `Files`), with their imports of other
  runtime files pointing at the runtime chunk; the chunks that use them import those symbols from them
  (`import { LineChart } from "../charts.[hash].js";`, and from the entry
  `import { BrowserFiles } from "./files.[hash].js";`) instead of binding `$k.native`
  ([runtime.md §1](runtime.md#1-status-and-scope)). The `k.drag` components and `KeyboardShortcut`
  render on the server through `$s.natives["k.drag.…$ssr"]` and `$s.natives["k.ui.KeyboardShortcut$ssr"]`.
- **Stdlib declarations** the client or server code references are emitted like project code when they
  are non-native enums or structs (`HttpError`, `HttpStatus`, `HttpRequest`). A stdlib struct's memberwise
  initializer fills in the defaults its fields declare, lowered from the stdlib's own source
  (`LineSeries(name:points:)` has `color = null, dashed = false, showsPoints = false`; `HttpRequest`'s
  empty maps are `HashMap`s, which take any key), as a project struct's does; before, a stdlib struct got only the fields a call passed. A stdlib symbol that no item
  defines (a `@Native` member without a generated body) is bound per chunk: `const StorageKey$init =
  $k.native("k.storage.StorageKey$init")` in the client ([runtime.md §12.1](runtime.md#121-services-in-the-browser)),
  `$s.natives[…] ?? $s.missingNative(…)` on the server.
- **Entry HTML**: the server renders the shell; a static-only app gets a generated `index.html` that loads
  `runtime` and `app`.
- **Printing**: development output is indented with 2 spaces, one statement per line; release output
  omits whitespace and comments (*deferred*: renaming locals), but keeps a space between a binary `-` or
  `+` and a right operand that starts with the same sign (`a- -b`, never the `a--b` that JS reads as a
  decrement), and spaces around a word operator (`e instanceof Error`, `"k" in o`), which would otherwise
  run into the names beside it. The runtime's own files are compacted by `k_runtime::compact` instead
  ([runtime.md §2](runtime.md#2-modules-api-and-size-budget)). Numbers print in shortest round-trip form;
  strings with `"`.

---

## 18. Source maps

- Every emitted JS file has a v3 source map (`file.js.map`, referenced with `//# sourceMappingURL=`)
  mapping statements and call expressions back to `.k` spans; `sources` are project-relative paths,
  `sourcesContent` is included in development builds and omitted in release builds.
- Server stack traces are mapped through the server bundle's maps by `k_server` when a panic is logged
  or (in development) shown on the 500 page ([server.md §5](server.md#5-request-lifecycle)).
- CSS source maps are *deferred*.

---

## 19. Output layout of `dist/`

```text
dist/
  app                         the server executable (absent for a static-only app)
  manifest.json               the Manifest, pretty JSON
  public/
    index.html                static-only apps only
    assets/
      runtime.3f2a1b9c.js     hashed client files: chunks, stylesheets, source maps
      app.9c1d2e3f.js
      app.5a6b7c8d.css
      pages/product.0a1b2c3d.js
      *.map
```

- `dist/app` is a copy of the server runtime executable with the server modules, assets, manifest and
  migrations appended as a trailer (format: [server.md §12](server.md#12-packaging-distapp)). v0 uses the
  `k` binary itself as the runtime executable; a slim runtime binary without the compiler is *deferred*.
- `dist/public` holds the same client assets for teams that serve them from a CDN.
- Hashed files are served with `Cache-Control: public, max-age=31536000, immutable`.
- `manifest.json` is for tooling and humans (`k routes`, deploy scripts); `dist/app` embeds what it needs
  (the config table and the session-free endpoint templates).
- Every app links one stylesheet, `app.[hash].css`, which starts with the base rule `[hidden] { display:
  none !important; }`; its name hashes the file as shipped (minified in release builds).
- A stdlib module of the server bundle defines every symbol other modules import from it: a `@Native`
  member without a body is `$s.natives["k.crypto.Uuid$random"] ?? $s.missingNative(…)`, so the bundle
  always loads and only an unimplemented member's request fails.

---

## 20. Performance budgets

| Area | Budget |
|---|---|
| Lowering + printing, single-threaded | ≥ 200,000 lines of K per second (≈ 7 MB/s) |
| Cold `k build`, 100,000-line project, 8 cores | ≤ 2.5 s including checking ([architecture.md §12](architecture.md#12-performance-budgets)); codegen ≤ 0.6 s of it |
| Generated client JS for a page | ≤ 1.5× the size of the equivalent hand-written signals code, before compression |
| Warm re-emit after a body edit | only the edited declaration's module per side is re-emitted (no whole-program pass) |

They are measured by `crates/k_codegen/benches/codegen.rs` (criterion) and enforced by
`crates/k_codegen/tests/perf_budget.rs` (`scripts/bench-check.sh --codegen`) over the generated projects of
`crates/k_sema/benches/project/`, which also checks that codegen time is linear in the project size
(10k → 100k lines ≤ 14×). Measured 2026-11-07 ([performance.md §11.3](performance.md#113-budget-status)):
500,000 lines/s single-threaded (200 ms for 100k lines, 4.8 MB of output), ratio 10.7×, cold 100k-line build
314 ms with codegen 201 ms of it. The generated-size and warm re-emit budgets are not measured yet.

Code generation reads the checker's facts through point queries (`type_info_at`, `resolve_at`) once per
syntax node, so those must be O(1) in the project size; [performance.md §11.4](performance.md#114-changes-made-while-setting-up-the-benchmarks)
records the three per-call file scans that made codegen quadratic before the benchmarks existed.

---

## 21. Testing

### 21.1 The test bundle (`k test`)

`Entry::Tests` (with `Environment::Test`; `suite.rs`) generates the bundle `k test` runs: the project's
client bundle with test code included, and its server bundle, which the runner loads into the same
isolate.

- **The server half.** Every declaration is lowered for the server too (test groups excepted), and the
  server entry `k:/app/$server.js` is the app's with three differences: it exports `$test` instead of
  calling `serve` (`{spec, app: createApp(spec), groups, reset}`), `$process` is a `let` that `reset`
  replaces, and `spec.container(st)` uses the test's group container when the request carries a test
  (`st.test`). Each group gets a server container `$S<N> extends $Request` (`groups[N]` builds it) with
  the group's rules that a server container can hold, each bound once per test (`$s.perTest`); a test's
  `k.test` doubles and fakes (`RecordingEmailProvider`, `TestHttpClient`, test-only services) are also one
  per test, `Clock` and `TestClock` are `$s.clock(this.$r)`, the test's virtual clock, and `TestClient` is
  `$s.testClient(…)`. A config gets its memberwise constructor, so a test can bind
  `AdminConfig.sample`.
- **Which side an injection comes from** (`suite::server_side`): a group's `@Inject` field reads the
  server container (`$k.$test.server().Name`) for a server-only declaration (repositories, configs,
  protocol implementations, `.server` services), the server stdlib services (`SessionManager`,
  `TestClient`, `PublicUrls`, the email and job doubles), and a fake a rule binds where server code uses
  its target (directly, through `@Inject`, or behind a protocol whose implementation injects it); every
  other field, protocols included, reads the browser container. A rule whose target is server-side
  applies only on the server.
- **Server names in browser code.** Test code is lowered for the browser; a server-only declaration it
  names (an entity, `Order.sample`) is imported from the server module (`from_server` in
  `bundle::client_files`), and a server-only stdlib member it calls (`Hmac.sha256`) is answered by the
  kit's `native` from the server's natives. Payload-free enum cases are one value per tag
  (`unitCase`), so a case read back from the server is `===` the test's.

- Every test group lowers to a class `$group<N>` (test groups get unique JavaScript names, since a group is
  usually named after what it tests): its fields lowered like a service's over the test's container
  (`@Inject` fields read the container, other fields are helpers), and each `test "…"` an async method
  `$t<i>($c)`.
- The entry builds the browser containers with every service of the program reachable, the types the
  groups inject added, the stdlib's `@TestDefault` for an unbound interface (`MemoryStorage` for
  `LocalStorage`, `TestHttpClient` for `HttpClient`, …) and `Clock` as the container's one `TestClock`;
  test code is never a single-implementation binding. One `$G<n> extends $App` per group applies its `use`
  rules (a type operand reads that service from the container, so `@Inject let fake: FakeX` sees the same
  instance; a value operand is memoized). It exports `$groups`: name, file, container factory, class and
  tests (name, method, the range of the name), and `$pages`, the pages' typed URL matchers, for
  `Screen.currentPage`.
- All pages go into the shared chunk (the entry imports them). The runtime chunk is
  `export * from "k:runtime"; export * as $test from "k:testing/kit"; export { native } from …`, and a
  feature chunk re-exports its embedded module, so everything shares one runtime instance
  ([runtime.md §13](runtime.md#13-testing)).
- `expect` evaluates the operands of a comparison (or of `not`) once into temporaries, in order (logical
  operators are not split, so short-circuiting is kept), and calls `$k.$test.fail` with the condition's range,
  its text and lazy renderings of the operand values (`text_fn`; strings quoted). `expect throws P { … }`
  matches `P` as a `catch` clause would (panics pass through) and calls `failThrows` with what was thrown
  instead, if anything. In other entries both keep their plain panics.
- `render(V(…))` builds `V` exactly as a view would (`Tree::call_node`: props, modifiers, slots) and passes
  its renderer to `$k.$test.render`; a page (`render(P(id: 1).tab(.x))`) gets its parameters, chained ones
  included, as props, without its layout, and the page's URL, which the browser is then on. A chained
  parameter written as an implicit member (`.units(.imperial)`) is the case of the parameter's type. Plain
  `@Prop`/`@Param` arguments are evaluated once, when the test renders; bindings, slots and handlers stay
  functions.

Fixed while making the examples' tests pass: an omitted `@Stylable` prop is no class (it was read as a
function), an omitted optional `@Slot` is `nil`, a form control's computed `.label(…)` becomes its
`aria-label` (it was dropped, leaving the control unnamed), and a slot called with arguments (`row(item)`)
re-renders when they change, through `branch` with an index that flips on a new value, so a keyed item
renamed in place reaches the row (`tests/corpus/codegen/ui/**` cases for each).

Fixed while making the examples' server-side tests pass, each pinned first:

- `await` covers the whole expression to its right: in `await next() == 2`, `await a()?.x` or
  `await items()[0]` each call is awaited where it is evaluated (it awaited the comparison, a promise's
  member or element) (`run/async/await-covers-a-comparison`).
- A qualified case pattern (`expect throws CounterError.atLimit`, `catch` clauses alike) names its last
  name: it was read as the type's name and matched any error (`k_cli` fixture `throws`).
- A test group named like the declaration it tests resolves its own members by their labels first: a
  helper `signIn(email:password:)` was lowered with the service's `signIn(request:)` signature
  (`sema/app/tests/group-named-like-its-service`, the `regressions` fixture's `Tally`).
- `Page.signIn` (a case whose parameters are all optional) is the case with its defaults, a value; it was
  the constructor function (`regressions` fixture's `Overview`).
- A user type implementing `HttpClient` gets `fetchJson` (`run/interfaces/stdlib-default-method-on-a-user-type`,
  `codegen/lang/stdlib/service-implementing-http-client-gets-fetch-json`).
- `HttpResponse` builds and reads alike on both sides through the `k.http` natives of `http.js`, a
  feature chunk in the browser (`run/http/built-response-reads-like-a-received-one`).
- `structuredData(…)` in `metadata` reaches the document head as JSON-LD
  (`codegen/ui/views/structured-data-reaches-the-head`); `Endpoint` values have URLs (`Endpoint$href`) and
  `PublicUrls.absolute(endpoint:)` builds them (`codegen/ui/routing/endpoint-urls-are-built-on-the-server`);
  a `StorageKey` is a value on the server (`run/regressions/storage-key-is-a-value-on-the-server`).

### 21.2 Test suites

- **Golden output**: `tests/corpus/codegen/**` ([testing.md §3.9](testing.md#39-the-codegen-suite)): exact
  `client.js`, `server.js`, `css`, or `.expect` assertions (including the split checks of §16).
- **Behaviour**: `tests/corpus/run/**` ([testing.md §3.10](testing.md#310-the-run-suite)), the main suite for
  language semantics: a program's log output and result after running in V8.
- **End to end**: `tests/e2e/<project>/` served over HTTP ([testing.md §8.2](testing.md#82-server-end-to-end-tests)).
- **Running bundles**: `crates/k_server/tests/client_bundle.rs` loads a generated client bundle
  (runtime, shared chunk) as ES modules in V8 with the fake DOM, renders a component and clicks it, in
  development and release builds, and runs its server renderer.
- **API and robustness**: `crates/k_codegen/tests/codegen.rs` checks refusal of erroneous programs,
  determinism, and that the four example apps generate under every profile with nothing server-only in
  their client files.
- **Unit tests** in `k_codegen` for hashing, the printer's escaping and precedence, and CSS constant
  folding. An `arbitrary`-driven property test that lowering never panics on generated programs is
  *planned*.
