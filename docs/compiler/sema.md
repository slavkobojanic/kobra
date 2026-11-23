# Semantic Analysis

`k_sema` turns the syntax trees of a project into a checked program: the module graph, resolved names,
the HIR, types, effects, control flow, and the framework rules that make Kobra's guarantees hold. This
document is the design of that crate and the source of truth for **what is checked, where, with which
code, and in which release**. The language is specified in [`docs/design`](../design/README.md); the
grammar in [grammar.md](grammar.md); diagnostic conventions and every code's primary span in
[diagnostics.md](diagnostics.md); how to write tests in [testing.md §3.8](testing.md#38-the-sema-suite).

Contents:

1. [Status and scope](#1-status-and-scope)
2. [Public API](#2-public-api)
3. [Query architecture](#3-query-architecture)
4. [Project model](#4-project-model)
5. [The standard library surface](#5-the-standard-library-surface)
6. [Module graph and visibility](#6-module-graph-and-visibility)
7. [Item trees, def maps and name resolution](#7-item-trees-def-maps-and-name-resolution)
8. [HIR](#8-hir)
9. [Type system](#9-type-system)
10. [Effects](#10-effects)
11. [Control flow](#11-control-flow)
12. [Framework layer](#12-framework-layer)
13. [Diagnostic codes](#13-diagnostic-codes)
14. [Grammar implications checklist](#14-grammar-implications-checklist)
15. [Performance](#15-performance)
16. [Implementation plan](#16-implementation-plan)

---

## 1. Status and scope

**Status:** the public API (§2), the project model (§4), the stdlib surface (§5), the corpus harness and
**rounds 1a and 1b of the checker** are implemented: the module graph, imports and visibility (§6), name
resolution and doc links (§7), types and inference (§9), effects (§10), control flow (§11) and the
framework layer (§12: annotations, components, views, styles, themes, reactivity, DI, layering, the
server/client boundary, routing, protocols, `@Load`, forms, configs, persistence and tests), with
`type_at`, `resolve_at` and `side_of`. How each round implements the design, and its remaining
leniencies, are in §16 ("Round 1a notes", "Round 1b notes"). Every `sema` corpus case passes; the suite
keeps the `.pending` mechanism for test-first cases of later stages.

**v0** is everything needed for the full-stack vertical slice of
[architecture.md §16](architecture.md#16-milestones): every rule below marked **v0** must be implemented
before v0 ships, with the code listed in [diagnostics.md](diagnostics.md#semantic-codes). **Deferred**
rules are designed here so the architecture accommodates them, but are not checked in v0; each names the
stage or milestone that picks it up. A deferred rule's code is assigned when it is implemented.

| Area | v0 | Deferred |
|---|---|---|
| Project model and files | layout, file rule, duplicates, misplaced declarations; workspaces, manifests and workspace dependencies (§4.1) | git packages (`k add`), aliases |
| Modules and imports | resolution, visibility, cycles, test-only modules, unused/duplicate imports; library modules of workspace dependencies | modules of git packages |
| Names | locals, parameters, members, declarations, prelude, implicit members | — |
| Types | everything in §9, explicit type arguments and covariance of the stdlib's read-only positions | variance annotations; higher-kinded uses |
| Effects | `async`, typed `throws`, `try`, exhaustive `catch`, error-generic parameters | — |
| Control flow | exhaustiveness, `default` rules, `guard`, definite initialization, jumps, `return` in lambdas, `for await` | unreachable-code warnings (except unreachable arms) |
| Components and reactivity | annotation table, members, `@Derived`/`@Effect` rules, `@Load` graph | the "reads after `await`" lint; "effect reads nothing" warning |
| Views | element table, attributes, events, `key`, content rules, components' props/slots/bindings | accessibility lints beyond required names |
| Styles and themes | property table, value grammar, classes, keyframes, tokens, overrides, variants | `@container`/`@layer` value checks |
| DI, layering, boundary | DI graph, scopes, layering table, state ownership, server/client classification | — |
| Routing | `Page`/`Endpoint` enums, `@Param`, `at`, URL conflicts, `requires`, endpoint pages; `routes.lock` (K8007, in `k_codegen`, §12.16) | — |
| Protocols | rpc `throws` rules, Codable payloads, server-only implementations; `remote` rules and `RemoteClient.at` (§12.8); wire names and field numbers in `rpc.lock` | streaming restrictions beyond `Stream` returns; streaming between services |
| Persistence | entity column types; migrations and `schema.lock` (K6004–K6006, K8004–K8006, in `k_codegen`, §12.12) | — |
| Forms | `.empty`, `form.value`, `@Format`, `@Email`, `@Matches` | — |
| Config | `@Env`, `Secret<T>` server-only, constraints on config fields | startup validation (runtime) |
| Jobs | `Job` services server-only, Codable payloads, no session in the graph, payloads vs `storage.lock` (§12.16) | — |
| Tests | `expect`, `expect throws`, `use` in tests, test-only visibility | — |
| Doc comments | intra-doc links | type-checking fenced `k` examples |
| Lockfiles | `storage.lock` keys, collections, key names and job payloads (K8002, K8003, K8008, K8009, K8010) and the lock the program implies, `rpc.lock` (K8001, K8002); `schema.lock` and `routes.lock` in `k_codegen` (§12.16) | — |

---

## 2. Public API

The API in `crates/k_sema/src/lib.rs` is final; later stages fill in its implementation.

| Item | Purpose |
|---|---|
| `AnalysisHost` | Owns a project's inputs (every file, by `ProjectPath`) and applies `Change`s as revisions. |
| `Change` | A batch of `set_file` / `remove_file` edits, applied atomically. |
| `Analysis` | An immutable, `Send + Sync`, cheaply cloned snapshot of one revision. |
| `Analysis::diagnostics` | Every diagnostic of the project, sorted (diagnostics rule 7). |
| `Analysis::file_diagnostics` | The diagnostics whose primary span is in one file. |
| `Analysis::type_at(file, range)` | The type of the expression, pattern binding or declaration name whose range is exactly `range`, rendered as Kobra type syntax. |
| `Analysis::resolve_at(file, range)` | What the name, member, element, attribute, CSS property, class, keyframes or token reference at `range` refers to (`Resolution`). |
| `Analysis::side_of(file, range)` | The server/client side of the top-level declaration whose name token is `range` (`Side`). |
| `Analysis::program()` | The read-only **program view** (`k_sema::view::ProgramView`) code generation consumes: every declaration and extension (kind, module, file and syntax range, annotations, resolved `implements`, extended type, function signature, constant type, side), every member (kind, flags, field type, signature with labels, defaults and parameter ranges), every generic parameter with its constraints, and extensions by extended type. Ids (`DeclId`, `MemberId`, `GenId`) index its tables for one revision. `ProgramView::resolve` maps a `Resolution` back to ids (members of extensions through the extended type; a member of a test group that shares its qualified name with the declaration it tests, `test Checkout { }` beside `service Checkout`, by its labels first, so a group helper is never taken for the service's function of the same name). Built once per revision at the end of the semantic pass, after the rest of the result is handed over (`view::ViewSlot`): the diagnostics don't wait for it, and a query that needs it before it is built waits. Signatures it resolves that no check needed report nothing. |
| `Analysis::type_info_at(file, range)` | Like `type_at`, but the structured `view::Type` (ids into the program view); unresolved inference variables are `Type::Error`. |
| `Cancelled` | Returned by every query once the host has moved to a newer revision. |
| `ProjectPath`, `FileKind`, `ModulePath`, `LockfileKind` | The project model (§4). |
| `stdlib` | The embedded standard library and its tables (§5). |

**Rendering conventions** (used by `type_at`, `resolve_at` and the corpus annotations):

- A prelude declaration is written by its simple name (`Int`, `List<String>`, `Page`); every other
  declaration by its fully qualified name (`shop.cart.CartItem`, `k.http.HttpStatus`).
- Optionals are `T?`; function types are `(Int, String) async throws k.http.HttpError -> Bool`
  (`async` and `throws E` only when present; `-> Void` is written); tuples are not in the language.
- Functions carry their labels Swift-style: `add(product:)`, `find(_:)`, `refresh()`.
- Unit types are their nominal names (`Length`, `Duration`); token categories are their own types
  (`Spacing`, `Radius`).
- An erroneous expression's type is `{error}`; an unconstrained inference variable that remains is
  `{unknown}` (it is also reported as K2002).

---

## 3. Query architecture

### 3.1 Decision: hand-rolled, salsa-style layered queries

`k_sema` uses **its own small query engine**, not the `salsa` crate. architecture.md §6 left the choice
open ("in the style of salsa"); this records it.

**Why not `salsa`:**

- Its current API is pre-1.0 and has been rewritten twice (the 2022 "new salsa" macros); the project's
  rule is few, well-known, stable crates, and salsa brings a proc-macro stack and several concurrency
  crates with it.
- Kobra's dependency structure is **coarse and static**: about ten query kinds with a fixed layering
  (§3.3). A generic dependency-tracking engine buys little over an explicit pipeline in which each layer
  is memoized per key and compared for early cutoff.
- Parallelism: Kobra needs deterministic **fan-out per layer** (parse all files, build all item trees, check
  all declarations) on a fixed thread count. That is a scoped-thread work queue (as `parse_all` already
  does), simpler and more predictable than salsa's parallel snapshots.
- Debuggability: each layer's inputs and outputs are plain data that tests can assert on.

**What we keep from salsa:** pure query functions over explicit keys; revisions; memoization with
`Eq`-based **early cutoff**; cancellation of stale snapshots; durability-like treatment of the stdlib
(parsed once per process, never invalidated). Because every query is a pure function of its key and the
previous layers, adopting salsa later would be a mechanical change, not a redesign.

### 3.2 Revisions, snapshots, cancellation

- `AnalysisHost::apply_change` creates a new `State` (an `Arc`) sharing every unchanged file's memoized
  results, bumps the revision, and stores it in an `AtomicU64` shared with snapshots.
- An `Analysis` holds an `Arc<State>`. Every query first checks that the shared revision still equals
  its own and otherwise returns `Err(Cancelled)`; long loops (per file, per declaration) re-check between
  units, so an editor keystroke stops stale work within one unit.
- Results computed for a snapshot are stored in the `State`'s memo tables (`OnceLock` per key), so a
  later snapshot of the same revision, or a later revision that shares the entry, reuses them.

### 3.3 Layers

Each layer is a function of the layers above it. Keys are stable ids (§3.4).

| # | Query | Key | Result | Recomputed when |
|---|---|---|---|---|
| 0 | inputs | `FileId` | text, `FileKind` | the driver changes the file |
| 1 | `parse` | `FileId` | `k_syntax::Parse` | its text changed |
| 2 | `item_tree` | `FileId` | position-independent declarations and signatures (§7.1) | its parse changed |
| 3 | `module_graph` | — | modules, their files, imports, cycles | the file set or any item tree's imports changed |
| 4 | `def_map` | `ModuleId` | names visible in the module | item trees of the module or of modules it imports |
| 5 | `signature` | `DeclId` | resolved signature: generics, fields, members, annotations | its item-tree entry or a def map it resolves against |
| 6 | `body` | `DeclId` | HIR + source map (§8) | its syntax changed |
| 7 | `infer` | `DeclId` | types of every expression and pattern | its body, or a signature it used (recorded per body) |
| 8 | global graphs | — | DI graph, route table, side classes, `@Load` graphs, theme tokens | signatures only |
| 9 | `check` | `DeclId` | diagnostics | its inference, or a global graph it participates in |

**Early cutoff:** a layer's result is compared with the previous revision's (all results are `Eq`). An
unchanged item tree stops propagation: editing a function body re-runs layers 1, 2 (equal), 6, 7 and 9
for that one declaration only, which is what makes the warm body-edit budget (30 ms) reachable.

**Dependency recording:** `infer(decl)` records the set of `DeclId`s whose signatures it read. A
signature change re-checks exactly the bodies whose set contains it.

### 3.4 Ids and interning

- `FileId` (`k_diagnostics`): stdlib files take ids `0..n`; project files follow in insertion order, and
  a path keeps its id for the host's lifetime.
- `ModuleId`, `DeclId`: interned from (module path) and (module, kind, name), so they are **stable across
  revisions** while the declaration exists, which keeps memo keys valid.
- `Name`: a `u32` symbol from a sharded, append-only interner shared by the host (names repeat heavily).
- `TypeId`: hash-consed type terms in a type interner; type equality is id equality.
- Inside a body, `ExprId`, `PatId`, `StmtId` are indices into the body's arenas (§8).

### 3.5 Parallelism and determinism

- Layers 1, 2, 6, 7 and 9 fan out per key over `Options::threads` scoped worker threads with a shared
  work index; layers 3, 4, 5 and 8 fan out per module or run once.
- Workers write only to their key's memo slot; shared structures are read-only within a layer.
- Diagnostics are collected per key and sorted at the end (file, start, end, code), so output never
  depends on the thread count. A test asserts serial and parallel runs are identical.
- Worker threads get a 4 MiB stack (the parser's nesting limit needs about 2 MiB in debug builds).

---

## 4. Project model

Implemented in `crates/k_sema/src/project.rs`. The layout is fixed ([24-projects.md](../design/24-projects.md)).

| Path | `FileKind` | Analysis |
|---|---|---|
| `k.toml` | `Manifest` | read by the workspace layer (§4.1): malformed → K8101 |
| `App.k` | `App` | parsed; holds the `app` declaration and `use` rules |
| `src/<dirs>/<Name>.k` | `Source { module, is_test }` | the module is the dot-joined directories |
| `src/<dirs>/<Name>.test.k` | `Source { is_test: true }` | same module; see test files below |
| `migrations/<name>.k` | `Migration` | not a module; holds one `migration` |
| `routes.lock`, `schema.lock`, `storage.lock`, `rpc.lock`, `k.lock`; `protocol.lock` (recognized, never read: folded into `rpc.lock`) | `Lockfile` | inputs to the lockfile checks (§12.16) |
| `.deps/<dir>/<path>` | the kind of `<path>`, sources only | a library mounted by the workspace layer (§4.1); its `k.toml` is mounted as an inert `Other` file |
| any other `.k` file | `Stray` | parsed, then K1001 **v0** |
| anything else | `Other` | ignored |

**v0 rules:**

- **K1001** a stray `.k` file (at the root other than `App.k`, directly in `src/`, nested under
  `migrations/`, or under directories that aren't identifiers).
- **K1002 / K1003 the file rule:** at most one `public` declaration per file, named like the file
  (`CartService.k` holds `public service CartService`). Extensions don't count. A `*.test.k` file has no
  public declaration; its file name is `<Name>.test.k` for any `<Name>`.
- **K1004** two declarations with the same name in one module (across its files); test files have their
  own namespace layered over the module's (below).
- **K1005** `app` outside `App.k` (or a second one), `migration` outside `migrations/`, anything but one
  `migration` in a migration file, and `test` declarations outside `*.test.k` files.
- A project with `App.k` is an app; without it, a library package.

**Test files.** Declarations in `*.test.k` files are visible only to the test files of the same module.
A test file sees its module's declarations, test-only modules and `k.test`. `test Name { … }` names are
free identifiers (they need not name a declaration), several groups per file are allowed, and they
don't take part in the K1004 namespace.

### 4.1 Workspaces

**v0**, design [40-workspaces-and-services.md](../design/40-workspaces-and-services.md);
`crates/k_sema/src/workspace.rs` (public: `k_sema::Workspace`, `k_sema::workspace::parse_manifest`).

- **A workspace** is a root `k.toml` with `[workspace] members = [...]`, member directories relative to the
  root (no `..`, no hidden directories, not nested in one another; a root has no `[package]`). A project
  outside every workspace is a workspace of one member whose directory is empty, so every driver plans
  the same way.
- **`Workspace::from_files`** takes every file below the root by its path relative to the root and plans:
  the members (sorted by directory), each with its own files, its direct dependencies and `libraries`,
  every library it depends on transitively in dependency order. Manifests are parsed here: malformed TOML,
  a missing `[package] name` or `k`, a dependency that is neither a git tag nor `{ workspace = true }`, a
  bad `members` entry, a duplicate package name → **K8101**; `{ workspace = true }` naming no member, used
  outside a workspace, or a git tag (not resolved yet) → **K8102**; a listed member without `k.toml` →
  **K8103** (at the `members` string); a dependency cycle (Tarjan's SCC over member edges, self-edges
  included) → **K8104** at every edge in the cycle, with the cycle as a note; a dependency on an app →
  **K8105**; a module declared by two packages of one member's build → **K8106** at the dependency entry
  that brings the second in. Edges reported as K8104/K8105 are left out, so the graph every analysis sees
  is a DAG and every member is still analysed.
- **Mounting.** `Workspace::analysis_files(member)` is the member's own files plus, for each library in
  `libraries`, its `k.toml` and non-test sources at `ProjectPath::mounted(dir, path)`
  (`.deps/<dir>/<path>`, `/` and `%` of `dir` escaped). `.deps` is hidden, so no file on disk can collide.
  A mounted file is classified by its inner path (`FileKind::Source` with the library's module), so a
  library's modules keep their own paths and are imported like the member's own (`import pay.Payments`);
  non-`public` declarations stay unreachable from the member (K1102). `ProjectPath::dependency()` tells
  mounted files apart; the DI checks use it (§12.6) and code generation takes a declaration's Protobuf
  package from its library's name.
- **Reporting.** `Workspace::report(analyses, display)` merges the diagnostics of several members'
  analyses and the manifest problems into one `SourceMap` of real paths (a mounted file is the library's
  own), dropping duplicates, so a library's diagnostic appears once however many members mount it. Only
  the problems of the analysed members' manifests (and the root's) are included.
- `Workspace::view_path(viewer, real)` and `real_path(member, path)` map between real paths and a
  member's analysis paths; the language server uses them to keep every mounted copy of a library file
  in sync.

**The driver** (`k check`, `crates/k_cli/src/sources.rs`) finds the projects named on the command line:
a path inside a directory with `k.toml` checks that whole project; a plain directory is searched for
projects, and its remaining `.k` files form an ad-hoc group without a manifest. A path in a workspace
checks the workspace (every member from the root, the member otherwise), each member with its libraries,
through `Workspace::report`. Manifest checks belong to the workspace layer (K8100–K8199), not to the
analysis, which never reports a missing `k.toml`.

---

## 5. The standard library surface

Implemented in `crates/k_sema/src/stdlib/` and `crates/k_sema/stdlib/`.

### 5.1 Decision: embedded Kobra source

The stdlib's declarations are **Kobra source files embedded in the binary** with `include_str!`:
`crates/k_sema/stdlib/k/<module>/<Name>.k`, one public declaration per file named like the file, exactly
as in an app. They are parsed by the same parser at first use (`Stdlib::get`, once per process; about
two hundred small files, a few milliseconds in total) and checked by the same checker, so the standard
library is documented, reviewable Kobra and the checker has no second declaration format.

- **Bodies the runtime provides are marked `@Native`** and omitted (`@Native func now() -> Date`); an
  `init` keeps an empty `{}` because the grammar requires a body. Native bodies are trusted: their
  signatures are checked, their absence of a body is not an error in stdlib files only.
- **`k.internal`** holds compiler-facing annotations (`@Native`, `@Annotation`, `@Platform`,
  `@ServerOnly`, `@ClientOnly`, `@Generated`, `@Literal`, `@TestDefault`, `@Guard`, `@Dependency`,
  `@DeclarationName`). Every stdlib file sees it without an import; user code can't import it (K1101 as
  if it didn't exist).
- **Generated declarations** (`Page`, `Endpoint`, `ThemeVariant`, a component's `Classes`) are declared
  in the stdlib as `@Generated` shells; the checker fills their members from the program (§12.10).
- `crates/k_sema/src/stdlib/files.rs` lists every file; a test fails if the list and the directory
  differ. Other tests check that every file parses without diagnostics, follows the file rule, documents
  every public declaration and member, and that the tables below name existing declarations.

### 5.2 Layout

| Module | Contents (selection) | Design |
|---|---|---|
| `k.core` (the prelude) | `Int`, `Double`, `Decimal`, `Bool`, `String`, `Bytes`, `Url`, `Void`, `Optional`, `Result`, `List`, `Map`, `Set`, ranges, `Stream`, `Binding`, `Builder`, `View`, `Error`, `Equatable`, `Hashable`, `Comparable`, `Codable`, `Copyable`, units (`Length`, `Duration`, `Angle`, `Percent`, `Color`, `Spacing`, `Radius`, `FontSize`, `Font`), `Date`, `DateStyle`, `NumberStyle`, `Money`, `Currency`, `RoundingRule`, `Secret`, `ConfigValue`, `Environment`, `Page`, `Endpoint`, `ThemeVariant`, `MediaCondition`, `Event`, `Classes`, framework annotations (`@Prop`, `@Binding`, `@State`, `@Derived`, `@Effect`, `@Load`, `@Emits`, `@Slot`, `@Stylable`, `@Inject`, `@Param`, `@Scope`, `@Env`, `@Get`/`@Post`/`@Put`/`@Patch`/`@Delete`, `@Layout`, `@NoLayout`, `@Rendering`, `@Offline`, `@SignInPage`, `@Breaking`), `LoadState`, `onCleanup`, `untracked`, `global` | 08, 20 |
| `k.ui` | `Theme` (with `apply(_:)`), `Router`, `Stack`, `Grid`, `Spacer`, `Icon`, `IconName`, `ErrorBoundary`, `VisuallyHidden`, `Locale`, `AriaRole`, `ButtonType`, `ImageLoading`, `Axis`, `MetadataEntries`, `StructuredData` | 04, 05, 07, 22 |
| `k.http` | `HttpClient`, `FetchHttpClient`, `HttpRequest`, `HttpResponse`, `HttpMethod`, `HttpHeaders`, `HttpBody`, `HttpStatus` (enum with `.other(Int)`), `HttpError`, `DecodingError`, `IncomingRequest`, `WebSocketClient`, `StandardWebSocketClient`, `WebSocketRequest`, `WebSocket`, `WebSocketMessage`, `WebSocketState`, `WebSocketError` | 32, 27, 44 |
| `k.time` | `Clock` (a stdlib service: `now`, `sleep(for:)`, `every(_:_:)`, `after(_:_:)`), `TimerHandle`, `TimeZone`, `TimeOfDay`, `Weekday`, `Backoff` (the channels' reconnect delays) | 35, 44 |
| `k.rpc` | `RpcFailure`, `RpcOnlyError`, `Broadcast`, `RemoteService`, `RemoteClient`, `ServiceAuthenticated`, `ServiceCaller`, `ServiceConfig` | 11, 28, 29, 40 |
| `k.auth` | `Authenticated`, `Role`, `Session`, `SessionManager`, `SessionConfig`, `SessionUser`, `SessionUserId`, `SessionRole`, `AuthUserId`, `AuthRole`, `RateLimit`, `Rate`, `GuardRejection` | 26 |
| `k.storage` | `LocalStorage`, `BrowserStorage`, `StorageKey`, `StorageReadError`, `StorageWriteError` | 36 |
| `k.server` | email (`Email`, `EmailProvider`, `SmtpEmailProvider`, `SmtpConfig`, `SmtpSecurity`, `EmailError`), jobs (`Job`, `JobQueue`, `DatabaseJobQueue`, `JobConfig`, `JobError`, `JobKind`, `@JobName`, `@Retry`, `@Schedule`, `ScheduleTime`), `ServerConfig`, `PublicUrls` | 30, 31, 12 |
| `k.crypto` | `Hash`, `Hmac`, `Mac`, `Password`, `SecureRandom`, `Uuid` | 33 |
| `k.charts` | `LineChart`, `LineSeries`, `ChartPoint`, `ChartScalable`, `ChartTokens` | 37 |
| `k.db` | `Database`, `DatabaseConfig`, `Query`, `Schema`, `TableBuilder`, `ColumnBuilder`, `ColumnType`, `ColumnValue`, `@Id`, `@Index`, `@Unique`, `@Relation`, `@Version` | 19 |
| `k.validation` | `@Form`, `Form`, `Validatable`, `ValidationError`, `FieldError`, `FieldName`, `Rules`, `@Check`, `@Email`, `@Format`, `@Length`, `@Matches`, `@NotBlank`, `@Range` | 15 |
| `k.encoding` | `Json`, `@JsonKey` | 20 |
| `k.text` | `EmailAddress` | 31 |
| `k.observe` | `Log` | 38 |
| `k.test` (test-only) | `TestClock`, `TestClient`, `TestHttpClient`, `TestWebSocketClient`, `TestWebSocketServer`, `TestRemote`, `MemoryStorage`, `RecordingEmailProvider`, `RecordingJobQueue`, `InlineJobQueue`, `Screen`, `Element`, `Key`, `render` | 21, 40 |

Module names follow the design documents: email and jobs live in `k.server`, money and config values in
`k.core`, testing support in `k.test`. The examples' imports are pinned by
`crates/k_sema/tests/examples.rs`: every `import k.…` in `examples/` must name a public stdlib
declaration. The stdlib grows with the phases that need it.

### 5.3 The prelude

Every file sees, without an import, every public declaration of `k.core` plus `PRELUDE_REEXPORTS`
(`k.db.Id`, `k.db.Index`, `k.db.Relation`, `k.db.Unique`, `k.db.Version`, `k.encoding.JsonKey`,
`k.server.JobName`, `k.server.Retry`, `k.server.Schedule`, `k.ui.Router`). An explicit import shadows a prelude name.
Built-in elements are not declarations; they come from the element table.

### 5.4 Data tables

What has no Kobra declaration syntax is data in Rust, each with consistency tests:

- **`stdlib::elements`**: every built-in view element with its required named arguments (in alternative
  forms, `link(to:)` or `link(url:)`), chained attributes (generated per element, not registered
  modifiers), events with payload types, allowed content, accessibility category, plus
  `UNIVERSAL_ATTRIBUTES` and `HTML_NAMES` (HTML name → Kobra element hint for K3101).
- **`stdlib::css`**: every CSS property by its camelCase Kobra name with the CSS name, its value grammar in
  the CSS value-definition syntax, and the Kobra types its implicit members and interpolations resolve
  against; `DATA_TYPES` maps CSS data types (`<length-percentage>`) to Kobra types (`Length`, `Spacing`, …).
  `animationName` takes `<keyframes-name>`, which resolves against the component's keyframes.
- **`stdlib::annotations`**: for every annotation, where it may appear (`Target`) and which
  mutability it requires (`Mutability::Let` / `Var` / `Any`), which drives K3001 and K3002.

---

## 6. Module graph and visibility

**v0**, design [16-modules.md](../design/16-modules.md).

- **A directory is a module.** Modules are `ModuleId`s in the `module_graph` query, each with its files,
  test files and imports.
- **Imports name one declaration** (`import shop.catalog.Product`); there are no wildcards. Resolution:
  the longest prefix that is a module, then a public declaration of it. Unknown → **K1101** (with a
  suggestion); not public → **K1102**; a name imported twice → **K1106**; never used → **K1105**
  (warning, machine-applicable removal).
- **Cycles** between modules (Tarjan's SCC over import edges, test files excluded) → **K1103**, one
  diagnostic per import edge in the cycle, sorted, with the cycle as labels.
- **Test-only modules:** a module with a `testing` segment, and `k.test`, may be imported only from
  `*.test.k` files and other test-only modules → **K1104**. They are never part of `k build` output.
- **Visibility of declarations:** `public` (everywhere), none (the module), `private` (the file).
  A use beyond reach → **K1301**.
- **Visibility of members:** a member is visible wherever its declaration is; `private` restricts it to
  the declaration's body. `public` on a member → **K1302** warning with a removal fix. `public` on an
  `@Inject` field → **K1303**; `@Inject` fields are always private. Enum cases and rpcs can't be
  `private` (K1301 at the modifier).
- **Extensions:** `public extend T` members are visible wherever `T` is, without an import;
  `extend T` within the module; `private extend` → **K1304**; two visible extensions adding the same
  member name to one type → **K1305** at the second.
- **Writes are restricted separately from visibility:** service fields are writable only by the
  service's methods (K4006, §12.6); struct `var` fields by whoever holds a `var` copy.

---

## 7. Item trees, def maps and name resolution

### 7.1 Item tree

The item tree of a file is a **position-independent** summary: for every declaration its kind, name,
visibility, annotations (names and argument syntax), generic parameters and `where` clauses, and member
signatures (fields with type syntax and whether they have a default, functions with labels, `async`,
`throws` and return type syntax, enum cases with payloads, computed properties, `init`s, rpcs, theme
tokens, style class and keyframes names, `@Param`s, `at` templates, `requires` clauses); plus the file's
imports. Bodies, initializer expressions and view/style contents are excluded, except where a signature
depends on them (style class and keyframes *names*, theme token names). Spans are stored as syntax
pointers relative to the item, resolved only when reporting, so an edit inside a body never changes the
item tree.

### 7.2 Def maps

A module's def map binds names to `DeclId`s in layers, innermost first: the file's own private
declarations, the module's declarations, the file's imports, the prelude. Test files add their module's
test-file declarations and may import test-only modules. Lookups return the binding and its layer, so
"declared here and imported" conflicts are reported (K1106 for two imports; an import of a name the
module declares is K1106 too).

### 7.3 Scopes in bodies

Name resolution inside a body walks lexical scopes, innermost first:

1. block-local `let`/`var` bindings (visible after their declaration, K2306 for reads before
   initialization), pattern bindings of `if let`, `guard let` (visible after the `guard`), `match` arms,
   `catch` clauses and `for` patterns;
2. lambda parameters (`{ item => … }`, `{ (a: Int, b: Int) -> Int => … }`); Kobra has no implicit lambda
   parameter;
3. function parameters (internal names);
4. members of the enclosing declaration (`self` members are referenced unqualified; `self.x` is not
   needed);
5. the file's def map (§7.2).

An unresolved name → **K1201**, an unresolved type → **K1202**, both with an import fix when exactly one
visible declaration has the name. A member access on a known type with no such member → **K1203**.

### 7.4 Implicit member expressions

`.name` and `.name(args)` resolve **against the expected type** during type checking (§9.6), never during
name resolution: enum cases, static members and static functions returning the type, theme tokens of a
token category, generated cases of `Page`/`Endpoint`/`ThemeVariant`, and in styles a component's classes
(`.class(.title)`) and keyframes (`animationName: .fade`). No expected type → **K2007**; no such member →
**K2006**.

### 7.5 Doc-comment links

**v0**, design [39-doc-comments.md](../design/39-doc-comments.md). Only rustdoc-style intra-doc links
are checked: a code span in square brackets, ``[`Name`]``, ``[`Type.member`]``, ``[`k.http.HttpStatus`]``,
``[`find(id:)`]``. Plain backticks are code and never checked. A link resolves, in order, against the
members and parameters of the documented declaration, the file's imports, the file's package (the whole
app counts as one package, so another module's declaration resolves without an import, test-only
modules included), and fully qualified names. Unresolved → **K1901** (warning) at the name inside the
brackets. Type-checking fenced `k` blocks as doc examples is **deferred** (after v0).

---

## 8. HIR

Bodies (functions, initializers, field initializers, computed properties, views, styles, `metadata`,
`loading`/`failed`, tests, migrations, query lambdas) are lowered from the typed AST into an
**arena-allocated, id-based HIR**, one `Body` per declaration or member:

```text
Body        { exprs: Arena<Expr>, pats: Arena<Pat>, stmts: Arena<Stmt>, params: Vec<PatId>, root: ExprId }
BodySource  { expr: ArenaMap<ExprId, SyntaxNodePtr>, pat: ArenaMap<PatId, SyntaxNodePtr>,
              by_range: HashMap<TextRange, Node> }   (* the reverse map answers type_at / resolve_at *)
```

- Arenas are `Vec`s indexed by `u32` newtypes; bodies are independent, so they lower and check in
  parallel. Names are interned `Name`s; resolved references are `DeclId`/`MemberId`/local `PatId`.
- Every node keeps a syntax pointer back to its AST node; diagnostics resolve pointers to spans only
  when reported.
- **Desugaring** (each documented with its source mapping so diagnostics point at what the user wrote):
  `if let` / `guard let` → `match` on `Optional`; `try … else { }` → `match` on a `Result`; trailing
  lambdas → a final argument; chained optional props and element attributes (`.label("x")`) → named
  arguments of the element or component; string interpolation → a concatenation node; `bind x.y` → a
  `Binding` constructed from a place; `x?.y` → optional chaining; `for x in xs` → an iterator node kept
  high-level (codegen needs the original shape); compound assignment → assignment.
- **Views** lower to a tree of element, component, `if`, `for` (with key), `match` and text nodes;
  **styles** to rules with resolved selectors and declarations whose values are lists of CSS components
  and Kobra expressions.
- Missing syntax (an `ErrorNode`, a missing expression) lowers to `Expr::Missing`, typed `{error}`, so
  checking continues without cascades.

---

## 9. Type system

**v0** unless marked. Design [08-types.md](../design/08-types.md), [07-themes-and-units.md](../design/07-themes-and-units.md).

### 9.1 Types

- **Nominal declarations:** `struct` (value type), `class` (reference type), `enum` with payloads,
  `interface` (nominal, with default methods), `service`, `protocol`, `entity` (a struct-like value),
  `repository`, `component`/`page`/`layout` (values of `View` when instantiated), `config`, `theme`.
- **Built-ins:** `Int`, `Double`, `Decimal`, `Bool`, `String`, `Bytes`, `Void`, `Never` (internal: the
  type of `throw`/`return` expressions and the empty error), `Optional<T>` written `T?`, `Result<T, E>`,
  `List<T>`, `Map<K, V>`, `Set<T>`, ranges, `Stream<T>`, function types with `async` and `throws E`.
- **Generics** on types and functions with `where T implements I, …` constraints, checked at
  instantiation (**K2009**). No higher-kinded types and no variance annotations: the stdlib's read-only
  positions are covariant (§9.6), everything else invariant.
- **Type arguments in a type** give one type per parameter: `Box<Int, Int>` and `Int<String>` are
  **K2020** at the `<…>` list (`cx.rs`, `resolve_path_type`).
- **Optionals:** `nil` only for `T?` (K2001 otherwise); `if let`/`guard let`/`?.`/`??` narrow and
  unwrap; `T` converts implicitly to `T?`.

### 9.2 Structs, classes, initializers

- **Memberwise initializer:** every struct and entity has one named argument per stored field, in
  declaration order. **A field with a default value may be omitted** and takes its default; **omitting a
  field without one is K2008**, one diagnostic listing every missing field, with a fix inserting them
  ([08-types.md](../design/08-types.md), [15-forms-and-validation.md](../design/15-forms-and-validation.md)).
- A declared `init` replaces the memberwise initializer; classes have no memberwise initializer.
- **`let` fields and bindings are immutable** (**K2011**); a struct value bound with `let` can't be
  mutated through any path. Classes: `let` fixes the reference only.
- **Derivation with `implements`:** `Equatable`, `Hashable`, `Codable`, `Copyable` are derived for
  structs and enums when every field/payload conforms (K2009 at the `implements` entry naming the field
  that doesn't); `Copyable` adds `copy(field:…)` with every field optional. `List`, `Map`, `Set`,
  optionals and tuples conform when their elements do, and `Result<T, E>` is `Codable` when `T` and
  `E` are, so a `Result<T, Error>` can't cross the wire (K5503): an `Error` has no codec.

### 9.3 Interfaces and extensions

- Interfaces are nominal; `implements I` must provide every member without a default (**K2010**, fix
  inserts stubs). Default methods may call the interface's other members.
- Extensions add methods, computed properties and static members (no stored fields); their visibility
  follows §6. Number extensions such as `10.minutes`, `2.5.seconds`, `90.degrees`, `10.perMinute` are
  ordinary stdlib extensions on `Int`/`Double` returning unit types.

### 9.4 Units

`Length`, `Duration`, `Angle`, `Percent` and `Color` are nominal stdlib types; literals with units
(`16px`, `200ms`, `45deg`, `50%`, `#3B82F6`) have them. Token categories (`Spacing`, `Radius`,
`FontSize`) are distinct types convertible to `Length` where a length is expected. Same-unit arithmetic
folds at compile time (`8px * 2` is `16px`); mixed lengths (`100% - 2rem`) become a `calc()` term kept
for codegen; arithmetic across dimensions (`1s + 4px`) → **K2101**. Unit-less numbers scale units
(`2 * .md`).

### 9.5 Collections and literals

`[a, b]` is `List<T>`, `[k: v]` is `Map<K, V>`, `[]` and `[:]` take their type from the expected type
(K2002 when there is none). `Set<T>` is built from a list literal against an expected `Set`. Map keys
and set elements must be `Hashable` (K2009).

### 9.6 Inference

- **Bidirectional, local** type checking: each body is checked with expected types flowing down
  (`check(expr, expected)`) and synthesized types flowing up (`infer(expr)`). Expected types flow into
  implicit members, lambdas, collection literals, `nil` and unit literals.
- **Lambdas** take parameter and result types from the expected function type; unannotated lambda
  parameters without one → K2002. A lambda's value is its last expression.
- **Generic arguments** are inferred by unification at each call site, left to right over the
  arguments, with lambdas checked after the non-lambda arguments fixed their type variables. There is no
  global inference: `let` without a type takes its initializer's type.
- **Explicit type arguments** ([grammar RA-23](grammar.md#ra-23-explicit-generic-arguments-in-expressions))
  bind the fresh variables of the generics they are given to before any argument is checked: a
  function's (`decode<User>(json)`), a member's own (`Jwt.verify<Claims>(…)`, `names.map<Int> { … }`), a
  type's for its initializer (`KeyName<AesGcmKey>(name: …)`, which wins over the expected type), for a
  static member (`Sealed<Note>.parse(text)`, the `Meta` type carries them) and for a component call
  (`Picker<Int>(…)`, `body/view.rs`, `component_fields`). Each is checked against its parameter's
  constraints at the argument (**K2009**). A count that differs from the generics, or any type arguments
  on something that isn't generic (a local, a field of function type, a non-generic function), is
  **K2020** at the list, with a fix removing it in the second case; the generics are then `{error}`
  (`body.rs`: `Checker::explicit`, consumed by `instantiate_full`, `construct`, `decl_value`).
- **Calls:** arguments are matched by label (K2003 missing, K2004 unknown label); calling a
  non-function → **K2005**; a type mismatch anywhere → **K2001** with the expected type's origin as a
  label when it is in source.
- **Function subtyping:** a function value that isn't `async` or throws nothing is accepted where an
  `async` or `throws E` function type is expected (a test helper that never fails can be passed as a
  throwing `save` prop); its parameters are contravariant and its result covariant.
- **Covariance** (`Checker::assignable`, `named_assignable`): besides `T` → `T?`, a type converts to an
  interface it implements and a component to `View`. Inside a type, that holds in the read-only
  positions of the stdlib's types: the elements of `List`, `Set` and `Stream`, a `Map`'s values (its keys
  are invariant), both arguments of `Result`, an optional's value and a function's result. All other
  generic arguments (a project's own generic types, classes included) are invariant. **A `var` is
  invariant** (`Checker::check_var`): a `var` local's initializer, an assignment (except the first one of
  a deferred `let`), a `var` field's initializer and a `var` field's memberwise argument compare nested
  arguments by unification, so `var a: List<Animal> = dogs` is K2001 with a help, while a `let` takes it.
  **Representation:** a struct, enum or entity (or a generic parameter's value) held as a project
  interface gets an adapter, as `Error` a box and as `Codable` an existential (codegen's `coerce`);
  those conversions are made only for a value itself, an optional's value and a list's elements (also
  of an optional list), which codegen maps. Inside anything else, a `Set`, a map's values, a `Result`, a
  function's parameters or result, they are K2001 with a help (`Conv`, `changes_representation`).
  Classes, services and the stdlib's other interfaces keep the value as it is, so they convert
  everywhere covariance allows.
- **`try … else { }` where an optional is expected** widens a non-optional operand to its optional, so
  the fallback may be `nil`: `let v: T? = try Json.decode<T>(text) else { nil }`.
- **Lints (v0, warnings):** comparing a `Bool` with `true`/`false` → **K2012** with a fix.
- `{error}` unifies with everything and suppresses diagnostics that depend on it.

### 9.7 Stdlib-specific type rules

- `HttpStatus` is an enum with `.other(Int)`; nested patterns like `.upstream(.serviceUnavailable)` are
  ordinary enum patterns. A literal `.other(503)` whose code has a named case → **K2401** with a fix.
- `Secret<T>` is opaque: it has no `==` and no `Equatable`/`Hashable`/`Codable` (K2001 at the use);
  `reveal()` is server-only (§12.6). Interpolating it is allowed and renders `Secret(•••)`
  ([25-config-and-secrets.md](../design/25-config-and-secrets.md), the `Secret<T>` table), so logging a
  config can't print it.
- `Clock.now` is a plain read, not reactive (§12.3).
- `Money` ([34-money.md](../design/34-money.md)): `+` and `-` take two `Money` values and `Money * Int`
  scales by a quantity; any other operator or operand (`/`, `Money * Money`, `Money * Double`) is
  **K2013**, because a rate goes through `multiplied(by:rounding:)`. A literal with more significant
  decimal places than its currency (`1.005.usd`, `1.5.jpy`; trailing zeros don't count) is **K2404** at
  the whole `amount.code` expression. Adding or subtracting two `Money` operands whose currencies the
  checker can see and that differ (`1.usd + 2.eur`) is **K2407** at the whole binary expression; the
  result is still `Money`, so nothing cascades. An operand's currency is visible when it is a money
  literal (negated or parenthesized too), a `Money * Int` scaling of one, or a sum or difference of
  such operands (its left operand's currency). Anything else, a variable or a call, is known only at run
  time, where mixing panics (`body/literal.rs`, `check_mixed_currencies`).
- **Checked literals** ([20-standard-library.md](../design/20-standard-library.md#literals-and-parsing);
  `body/literal.rs`): a call that selects an initializer marked `@Literal` (`Url`, `Uuid`, `EmailAddress`,
  `TimeOfDay`, `StorageKey(name:)`, and `HttpStatus(code:)`, an enum's initializer checked in the enum
  branch of construction) must pass a plain string literal, or an integer literal (`-n` included) for
  `HttpStatus`; anything else is **K2403** at the argument, with a fix inserting `.parse` after the type
  name when the type has `static func parse(_:)`. A literal of the wrong kind is the type mismatch
  already reported. The literal is then validated by the type's parser in `crate::literals`, a port of
  the runtime's `parseUrl`, `parseUuid`, `parseEmail` and `parseTimeOfDay` (`std.js`; JavaScript's `\s`,
  `trim` and `.` semantics included) and of the status rule: a refusal is **K2404** at the literal,
  with the reason. `crates/k_server/tests/literals.rs` runs the compiler's and the runtime's parsers on
  the same fixed and generated inputs and requires identical results, and `literals.rs`'s unit tests pin
  the currency table to `money.js`.
- **Storage keys** ([36-local-storage.md](../design/36-local-storage.md#keys)): a `StorageKey(name:)` in
  the app's non-test code whose name an earlier key (in file path order) already has is **K2405** at its
  name literal, with a label at the earlier one (`locks.rs`, `duplicate_keys`).
- **Indexed collections** ([52-indexed-collections.md](../design/52-indexed-collections.md#declarations);
  `collections.rs`): every `Collection<Key, Value>` and `CollectionIndex<Field, Value>` constant or field
  is found by its type. A computed `name:` or `collection:` is **K2403**; a second non-test collection
  with an earlier one's name, or index on an earlier one's collection and field, is **K2410** at its
  literal with a label at the earlier one; a `key:` or `field:` that isn't a literal, names no stored
  field of the value struct, has another type than declared (an optional key never; `Field?` for an
  index), or isn't a key type (`String`, `Int`, `Uuid`, `Date`, a payload-free enum) is **K2411**, as is a
  `Collection(…)` or `CollectionIndex(…)` call in non-test code outside a constant's or field's
  initializer; an index naming no collection of its value type is **K2412**. The pass returns the
  collections' schema (`Analysis::collections`: each key and index by JSON key and kind, test-only ones
  marked), which code generation compiles into the browser's `BrowserCollections` and `k test`'s
  `MemoryCollections`.
- **Drag kinds** ([41-drag-and-drop.md](../design/41-drag-and-drop.md#kinds-and-payloads)): kinds with
  the same name are the same kind, so a `DragKind(name:)` constant or field (test code included) whose
  name an earlier kind (in file path order) has with another payload type is **K2406** at its name
  literal, with a label at the earlier one naming its payload. The same name with the same payload type
  is allowed; a payload type that doesn't resolve isn't compared (`locks.rs`,
  `conflicting_drag_kinds`).
- **Sums:** `List` has two `sum` overloads. `sum() -> T` adds the elements of a `List<Int>` or a
  `List<Double>` (the empty sum is `0` or `0.0`); `sum(in:_:)` sums `Money` in a currency. Because no
  interface expresses "Int or Double", `sum()` is declared unconstrained in `List.k` and the checker
  supplies the constraint: on a `List<Money>` it is **K2402** with a fix to `sum(in:_:)`, on any other
  element type **K2009**, both spanning `sum()`; the call keeps the element type, so nothing cascades.
  An element type that is still an inference variable or `{error}` is not checked.

---

## 10. Effects

**v0**, design [09-errors.md](../design/09-errors.md).

- **`async`** is part of a function type. A call to an async function, and a repository **query**, must
  be awaited (**K2204**; a query passed or returned without `await` too, [19-persistence.md](../design/19-persistence.md));
  `await` outside an async function, effect or handler → **K2205**.
- **Typed `throws E`:** a function declares exactly one error type (an enum implementing `Error`, or
  `Never` when absent). Every expression that can throw must be inside a `try` (**K2201**, fix inserts
  `try`); a `try` that covers nothing that can throw → **K2202** (warning). `try` covers its whole
  operand expression, like Swift.
- **Propagation:** an error thrown in a function must be caught or be the function's own `throws` type
  (**K2203**). Errors are wrapped at layer boundaries, never unioned.
- **Exhaustive `catch`:** the `catch` clauses of a `do` are checked with the same pattern-matrix
  algorithm as `match` (§11.1) against the thrown type; missing cases that don't propagate → **K2206**.
  A case without its payload (`catch .rpcFailure`) matches any payload. `catch let error` binds,
  `catch _` doesn't; an unused `catch let x` → K2304.
- **Error-generic parameters:** a parameter typed `() async throws E -> T` with `E` a type parameter
  makes the call `throws E`, `E` inferred from the block passed; a block that can't throw makes `E`
  `Never`, so the call needs no `try`.
- **`try … else { }` and `Result.of { }`** convert to values; there is no `try?` or `!`.
- **Effects and handlers can't throw:** an `@Effect` function or an event handler declared `throws` →
  **K3012**.
- **Panics** (index out of range, failed `assert`, unwrapping `nil` through stdlib APIs, server
  infrastructure failures) are not part of any type and are not checked.

---

## 11. Control flow

**v0**, design [08-types.md](../design/08-types.md), [09-errors.md](../design/09-errors.md).

### 11.1 Exhaustiveness

`match` and `catch` use one **pattern-matrix** algorithm (Maranget's usefulness): constructors are enum
cases, `Bool` values, `Optional`'s `nil`/some, a tuple (its type's single constructor, the elements its
fields: `case (0, true)` covers only that pair), literal sets (never exhaustive except `Bool`), and
ranges. `where` guards never count toward exhaustiveness.

- A non-exhaustive `match` → **K2301** at `match`, with a note listing missing patterns and a fix
  inserting arms. A non-exhaustive `catch` whose error can't propagate → **K2206**, likewise.
- An arm, `catch` or `default` that can never run → **K2302**; this includes a `default` when every
  case is already handled.
- **Default rules:** a top-level `case _` arm → **K2303** with a fix to `default`; an unused top-level
  catch-all `case let x` or `catch let x` → **K2304** with a fix to `default` / `catch _`. A
  pattern-level `_` inside a pattern (`.status(let s, _)`) is fine. An unused binding in `failed(let e)`
  follows the same rule as `catch let` (K2304).

### 11.2 Other rules

- **`guard … else { }` must exit** the enclosing scope (`return`, `throw`, `break`, `continue`, or a
  call returning `Never`) → **K2305**.
- **Definite initialization:** a `let x: T` without initializer must be assigned on every path before
  any read (**K2306**); a `let` assigned twice → K2011. Computed over the HIR's control-flow graph.
- **Return paths:** a function with a non-`Void` return type that can reach its end → **K2307**.
- **`return` inside a lambda body** → **K2308** (a lambda's value is its last expression; the fix
  removes `return` when it is the last statement).
- **`break`/`continue`** outside a loop, or crossing a lambda boundary → **K2309**.
- **`for await`** iterates only a `Stream<T>` inside an async function (**K2311**); a plain `for` over a
  `Stream` → **K2310** with a fix inserting `await`. A `key:` clause is not allowed with `for await`
  (parser).
- Unreachable statements after `return`/`throw` are **deferred** (warning, after v0).

---

## 12. Framework layer

### 12.1 Annotations and members

**v0**, design [01-principles.md](../design/01-principles.md), [04-components.md](../design/04-components.md).
An annotation is a stdlib declaration marked `@Annotation`; its arguments are checked like a call of its
initializer. The annotation table (§5.4) adds where it may appear and the required mutability:

| Annotation | On | Must be |
|---|---|---|
| `@Prop` | component field | `let` |
| `@Binding` | component field | `var` |
| `@State` | component/page/layout/service field | `var` |
| `@Derived` | component/page/layout/service field | `let` |
| `@Load` | page/component/layout field | `let` |
| `@Inject` | component/page/layout/service/repository/endpoint/test field | `let` |
| `@Param` | page field | `let` |
| `@Form` | page/component field | `var` |
| `@Slot` | component or layout field (a `View` builder) | `let` |
| `@Env` | `config` field | `let` |
| `@Effect`, `@Emits` | function of a component/page/layout/service | — |
| `@Stylable` | component | — |

Wrong `let`/`var` → **K3001** with a swap fix; an annotation where it doesn't apply → **K3002**.
Components, pages and layouts need exactly one `view` (**K3003**, except endpoint pages) and at most one
`style`, `metadata`, `loading` and `failed` block (**K3004**). An assignment member (`name = value`)
outside a `theme` → **K3303** (it parses in every member list, [grammar.md RA-32](grammar.md#ra-32-adopted-design-proposals)).

### 12.2 Components, props, slots, bindings

**v0.** A component call `Counter(count: bind n, label: "x") { … }` is checked against its props:
required `@Prop`s without defaults are named arguments (K2003); optional props (with defaults) may be
named or chained (`.size(.small)`), not both and not twice (**K3104**); `@Binding` props require
`bind place` of the same type and a `var` place (K2001 / K2011); `@Slot` props take the trailing block or
named builder arguments; `@Emits` events are subscribed with `.on(name: handler)` (**K3105** for unknown
names); `@Stylable` components accept `.class(...)` from the parent. A prop or event name that clashes
with a chained element attribute of the same element is K3104.

### 12.3 Reactivity

**v0**, design [03-reactivity.md](../design/03-reactivity.md).

- **Dependency analysis:** for each `@Derived` initializer, `@Effect` body, view expression and `@Load`
  initializer, the checker computes the set of reactive places read synchronously (fields of `@State`,
  `@Prop`, `@Binding`, `@Derived`, `@Load`, `@Form`, `@Param`, and services' `@State`/`var` fields,
  down to struct paths such as `user.address.city`); reads inside `untracked { }` and after the first
  `await` don't count. The sets are recorded in the HIR for codegen's signal wiring.
- A write to a `@Derived` field → **K3010**; a state write inside a `@Derived` initializer → **K3011**.
- `@Effect(on: [a, b])` names the dependencies explicitly; its elements must be reactive places (K2001).
- **`clock.now` is not reactive** ([35-time.md](../design/35-time.md)): reading it doesn't subscribe.
  Discarding the `TimerHandle` of `clock.every`/`clock.after` in an effect → **K3013** (warning).
- **Deferred:** "an effect that reads nothing" and "a read after `await` that looks reactive" lints.

### 12.4 Views

**v0**, design [05-views.md](../design/05-views.md). A lowercase call in a view is a built-in element
from the element table:

- unknown element → **K3101**, with the HTML hint (`a` → `link`, `img` → `image`, `div` → `box`);
- a required named argument missing → **K3102** (alternatives are listed);
- a chained attribute the element doesn't define (universal attributes included) → **K3103**; set twice
  → **K3104** (`.class()` twice included: use a list);
- `.on(name:)` with an unknown event → **K3105**; the handler's parameter must accept the payload;
- a view `for` without `key` → **K3106**;
- children the element doesn't accept (`Content::None`, `Only([...])`) → **K3107**;
- attribute and argument values are checked against the table's Kobra types (implicit members resolve
  against them: `.type(.submit)`).

Accessibility lints beyond required accessible names are **deferred**.

### 12.5 Styles and themes

**v0**, design [06-styles.md](../design/06-styles.md), [07-themes-and-units.md](../design/07-themes-and-units.md).

- **Typed properties:** the property must exist in the CSS table (**K3201**, with a camelCase fix for
  kebab-case names); its value is matched against the property's value grammar: CSS keywords are bare
  identifiers, literals must satisfy a data type, Kobra values (implicit members, `{interpolations}`) must
  have one of the property's Kobra types (**K3202**).
- **Camel-case keywords:** a keyword in a value, a function argument or a media feature value that
  contains `-` (`space-between`) is **K3214** with the machine-applicable camel-case fix (`spaceBetween`);
  it is then matched in that spelling, so the grammar doesn't report it again. The tables of
  `stdlib::css` spell keywords in camel case and expose them to tooling (`property_names`, `keywords`,
  `media_feature_keywords`).
- **Selectors** are built from classes, `&`, pseudo-classes and pseudo-elements, attribute selectors and
  `*`; element and id selectors are syntax errors (K0116, K0132; RA-8).
- **Class references:** `.class(.name)` / `.class([.a, .b when c])` must name a class of the
  component's style block (**K3203**); a class nothing references → **K3204** (warning). A
  `@Stylable` child's classes are passed from the parent's block.
- **Keyframes:** `@keyframes fade { from {…} 50% {…} to {…} }` declares keyframes scoped like classes.
  Keyframe selectors must be `from`, `to` or `0%`–`100%` (**K3207**); keyframe blocks hold declarations
  only (**K3208**); `animationName: .fade` must name keyframes of the same block (**K3205**); unused
  keyframes → **K3206** (warning). A parent can't reference a child's keyframes.
- **Theme tokens:** `color primary: Color = #3B82F6` declares a token of a category type; tokens are
  static members of their category (`.primary` against `Color`). Overrides (`name = value`) at the top
  level of a theme and in `variant name matches .condition { }` must name a declared token (**K3301**)
  and have its type; re-declaring a library token → **K3302** with a fix to the override form; `matches`
  takes a `MediaCondition`. The generated `ThemeVariant` enum has one case per variant.
- `@container` and `@layer` preludes are parsed but their values are **deferred**.

### 12.6 Services, DI, layering, server/client boundary

**v0**, design [10-services.md](../design/10-services.md), [23-app-structure.md](../design/23-app-structure.md).

- **The DI graph** (a global query over signatures): nodes are injectables (services, protocols,
  repositories, configs, stdlib platform interfaces, `Database`, `SessionManager`, `Clock`); edges are
  `@Inject` fields. An interface with exactly one implementation (stdlib defaults included) is injected
  automatically; with several, `use X for I [in .env, …]` rules in `App.k` choose (**K4002** when
  ambiguous, **K4001** when none). `use X for Y` requires `X` to be or implement `Y` (**K4010**). Cycles
  → **K4003**.
- **Every environment is checked.** For each of `.development`, `.test` and `.production` an interface
  is bound the way codegen binds it for that build (`k_codegen::server::binding`): the first `use` rule
  naming the environment or without `in`, else the single implementation. Sides, scopes (K4004),
  cycles (K4003), the job/session check (K4009) and K4007/K4008/K5504 follow **every** environment's
  choice, and a declaration's side is the union over them (the boundary is drawn per declaration). A
  program that breaks a rule in one environment only is an error; the diagnostic adds a note naming
  where, for example "in `.production`, `PaymentProvider` is `StripePayments`". Corpus:
  `tests/corpus/sema/app/services/per-environment/`.
- **Scopes** `.app` (default), `.request`, `.route`, `.component`, `.server` with the compatibility
  table: a service may inject only services that live at least as long (**K4004**, with the injection
  path). Stateless injectables (repositories, `Database`, configs, platform interfaces, `Clock`) work in
  every scope. A `.server` service or a job reaching `Session` → K4004 / **K4009**.
- **Layering table** (who may inject what): components, pages and layouts inject services (and `Clock`);
  endpoint pages inject services including server-only ones; services inject services, protocols and
  platform interfaces, and server-side services also repositories, configs, `Database` and
  `SessionManager`; repositories inject repositories and configs; configs nothing. Any other edge →
  **K4005** (components never inject protocols). `Clock` is a stdlib service, not a platform interface.
- **State ownership:** only a service's own methods write its fields (**K4006**); this includes tests
  writing a fake's fields.
- **Server/client classification** (`side_of`): server-only are entities, repositories, configs,
  endpoint pages, `Endpoint`, services that implement a protocol, are `.request`/`.server`, implement
  `Job`, or inject anything server-only; client-only are services that inject a client-only platform
  interface or service (`LocalStorage`, `Theme`); everything else is shared. Both → **K4008**. Using a
  server-only declaration from code that reaches the client → **K4007** with the path (this covers
  `config`, `@Env`, `Secret<T>` and `reveal()`). A client-only interface called from code that can run
  during server rendering (field initializers, `@Derived`/`@Load` initializers, views) → **K4012**; a
  server-rendered page reading a client-only service's state → **K4013** (warning; fixes
  `@Rendering(.client)` on the component, first, when the read is in a component, then on the app).
  Chaining `headers`, `secretHeaders` or `bearerToken` onto a `k.http.WebSocketRequest` in a declaration that
  isn't server-only (shared or client-only code may run in the browser, which can't send handshake headers)
  → **K4031** at the setter's name ([44-channels.md](../design/44-channels.md#raw-websockets-khttp)).
- **Client-rendered components** ([47-client-rendered-components.md](../design/47-client-rendered-components.md)):
  `@Rendering` applies to the app and to components (K3002 elsewhere). `@Rendering(.server)` on a
  component → **K4027**; `@Rendering(.client)` on a component of a `@Rendering(.client)` app → **K4029**
  (warning, machine-applicable removal); otherwise the component needs a `loading` block (**K3022**, with
  `@Load`'s message and fix, reported once). Its `loading` reads only `@Prop`s and static members of the
  component (top-level constants and functions are allowed): any other member read by name,
  `self.name` or a call → **K4028** at the read, labelled at the member. K4012 and K4013 don't apply in
  the browser-only regions of a declaration (`di::browser_regions`: a client-rendered component outside
  its `loading`, and the lambdas passed to a client-rendered component, i.e. its slot content) nor in a
  component every reference to which is in such a region, transitively
  (`di::browser_only_components`: references are found by name; every non-component declaration, every
  unreferenced component and every client-rendered component's `loading` are server-rendered roots).
  A client-rendered component's own `@Load` values and errors needn't be `Codable` (K3021, K3024);
  those of the components below it still must.
- **`@Worker` services** ([55-workers.md](../design/55-workers.md), `workers.rs`): `@Worker`
  (`k.concurrency`, imported) applies to services only (K3002 elsewhere). A worker service is
  client-only (`di::compute_side`), so server-only code injecting it is K4008 and calling it from
  code that can run during server rendering is K4012, with a message of its own (`body.rs`, which also
  covers a project's `@Worker` services besides the stdlib's client-only interfaces, and skips a
  declaration's calls to itself). It keeps no state and faces the page only through its calls:
  `@State`, `@Derived`, `var`, uninitialized and non-`private` fields and computed properties,
  `init()`, `@Effect`, `rpc`, non-`private` `static` functions and `@Scope` → **K4035**; any `@Inject`
  field → **K4036**. Each non-`private` instance method is `async throws E` and not generic
  (**K4037**), `E` is `WorkerOnlyError` or an enum with exactly one `workerFailure(WorkerFailure)` case
  (**K4038**, the rpc rule K5502's sibling), and its parameters, result and `E` are `Codable`
  (**K4039**). Corpus: `tests/corpus/sema/app/workers/`.
- **`use` operands:** `use <expression> for C` is allowed only in a `test` body and only when `C` is a
  `config` type (**K4011**); the `Type` form is allowed in `App.k`'s `app` and in `test` bodies
  ([grammar.md RA-35](grammar.md#ra-35-the-operand-of-use)).
- **`remote` rules** ([40-workspaces-and-services.md](../design/40-workspaces-and-services.md)): `use
  remote(Config) for P [in .env, …]` in `App.k` binds protocol `P` to a client of another service. The
  operand is the call `remote(Name)` (not K4011). `Name` must be a `config` implementing
  `k.rpc.RemoteService` (**K4025** at the name, which records its resolution); such a config needs `url:
  Url` and `token: Secret<String>` and may have `timeout: Duration` (**K4025** at the config's name or the
  field's type). `P` must be a protocol (**K4024**), not implemented by the app's own services (test fakes
  don't count; **K4024** naming the implementation) and declared in a library dependency, a mounted file
  (**K4024** for the app's own protocol). An injected protocol from a dependency that the app neither
  implements nor binds remotely is **K4023** at the field (apps only; test code exempt). Remote rules
  take part in K4015's environment coverage. **A remote client is server-only:** a protocol with a
  remote rule injects as `SERVER`, so injecting it from code that reaches the browser is K4007 with the
  path "… → pay.Payments (a `remote` client, which holds a service token, is server-only)" (also at an
  import when the file is all client code).
- **Tests' defaults:** in `k test`, `Clock` is `TestClock` without a `use` line (so `@Inject let clock:
  TestClock` is valid), and the stdlib's `@TestDefault`s apply ([21-testing.md](../design/21-testing.md)).

### 12.7 Routing

**v0** except `routes.lock`. Design [12-routing.md](../design/12-routing.md),
[27-endpoint-pages.md](../design/27-endpoint-pages.md).

- **`Page` enum:** one case per page, with the page's path `@Param`s as payload (`.product(slug:)`), and
  modifiers for query `@Param`s (`.signIn.returnTo(.notes)`); **`Endpoint`** likewise for endpoint
  pages (server-only).
- **URLs:** a page's default URL derives from its name and `@Param`s; `at "/p/{slug}"` overrides it and
  must mention each path `@Param` exactly once (**K5003**). Two pages with the same URL → **K5001**.
  `@Param` types must be routable (`String`, `Int`, `Uuid`, `Page`, enums without payloads, optionals of
  these) → **K5002**.
- **Endpoint pages** (pages with `@Get`/`@Post`/`@Put`/`@Patch`/`@Delete` handlers and no view) require
  an `at` clause (**K5004**); `requires Authenticated` or `Role` on them → **K5005**.
- **`requires` guards** must implement the stdlib `Guard` interface (**K5006**).
- **A layout's `requires` clause** is never checked by the server, so it is **K5014** at the clause, with
  a help to put it on each page that renders in the layout; its guards aren't resolved
  (`routing.rs`, `guard_on_layout`).
- **Layouts and app attributes:** `@Layout(X)`, `@NoLayout`, `@SignInPage(.signIn)` (a `Page` value),
  `@Rendering(...)` and `@Offline` (the `app` only, K3002 elsewhere) are type-checked like any annotation.
  `AppUpdates` injected in an app that isn't `@Offline` is the warning K4034 (`di::check`,
  [54-offline.md](../design/54-offline.md#updates)).
- **`routes.lock`** (the URL of every page and endpoint, and the redirects) is written by `k build` and
  compared by `k check`; both live in `k_codegen` (`routelock.rs`), next to the route table the server
  uses (§12.16).

### 12.8 Protocols and RPC

**v0**. Design [11-protocols-and-rpc.md](../design/11-protocols-and-rpc.md),
[28-rpc-wire-format.md](../design/28-rpc-wire-format.md).

- **Every rpc declares `throws`** (**K5501**, fix inserts `throws RpcOnlyError`); its error type is an
  enum with **exactly one `rpcFailure(RpcFailure)` case** or `RpcOnlyError` (**K5502**, fix inserts the
  case). The case is spelled `rpcFailure` because `rpc` is a hard keyword.
- Parameters, results and error payloads are `Codable` (**K5503**).
- A protocol is implemented by a `service … implements P`, always server-only (**K5504** otherwise);
  handlers repeat the rpc's signature (`throws` included), as an `rpc` or a `func`.
- **Only a protocol's rpcs are served:** an `rpc` member of anything but a protocol (a struct, a
  component, an extension, or a service) that no protocol in its declaration's `implements` clause
  declares is **K5508** at the `rpc` keyword, with a maybe-incorrect fix replacing it with `func`. The fix
  is left out when the rpc has a `requires` clause (a `func` can't have one, and K5507 reports the
  clause) or no body outside an interface. A declaration that names a protocol declaring the rpc but
  can't implement one (a struct) is K5504 instead (`rpc.rs`, `unserved_rpc`).
- **Guards are the protocol's:** a `requires` clause on an rpc that isn't a protocol member (a service's
  handler, or an rpc no protocol declares) is **K5507** at the clause. A label marks each protocol rpc the
  handler serves. The fix appends the guards those rpcs (and their protocols' headers) don't declare yet,
  compared by source text with whitespace collapsed, to each rpc's `requires` clause or after its
  signature, adds their imports to the protocol's file, and deletes the clause with the whitespace before
  it; it is maybe-incorrect, or machine-applicable when nothing moves. The fix also deletes the line of
  every import of the handler's file that only the clause used (each identifier token with the import's
  name, outside the imports, lies in the clause), so applying it leaves no K1105, except the imports of
  guards it moves to a protocol in the same file (`Cx::imports_only_in`, shared with K4014's fix). A
  protocol from a library or the stdlib gets a help instead of a fix, and so does an rpc no protocol
  declares, which is K5508 too (`rpc.rs`, `handler_guards`). Library files themselves aren't checked
  here.
- **A protocol or an rpc can't be generic** (**K5514** at its generic parameter list, `rpc.rs`,
  `generic_rpc`): `rpc.lock` records every rpc's concrete parameter, result and error types. A generic
  protocol's rpcs aren't checked further, and a generic rpc's handler isn't compared with it (no K5505).
- A protocol is injectable only into services (K4005). `Stream<T>` results are server streams
  ([29-server-streaming.md](../design/29-server-streaming.md)); client and bidirectional streaming are
  deferred with the wire format.
- There is no `protocol.lock`: `rpc.lock` (§12.16) records what deployed callers rely on, the wire
  names and field numbers included ([13-tooling.md](../design/13-tooling.md#lockfiles)).
- **Channels** ([44-channels.md](../design/44-channels.md); `channels.rs`, and `rpc.rs` for the
  messages). An rpc whose result is `Channel<C, S, E>` (`k.rpc.Channel`, a `@Native` class) is a channel:
  `C` and `S` are `Codable` (**K5503** at the type argument: "a channel's client/server message"), and `E`
  follows the rules of an rpc's error enum (**K5502**, with its fix, and **K5503** for its payloads). A
  `Secret` in `C` or `S` is K5503, since `Secret` isn't `Codable`; inside a struct declared `Codable`
  it is K2009 at the struct. Placement: a `Channel` or `Channel?` result of anything but a protocol's
  rpc or a service rpc serving one, and a `Channel?` result of those, is **K5509** at the return type; a
  type argument naming `Channel` (`List<Channel<…>>`, `Map<K, Channel<…>?>`, written anywhere) is
  **K5510** (a syntax walk of every project file). Sides, from the body checker: in a server-only
  declaration (`di::side`), a `Channel`'s `state`, `messages`, `send` or `close`, or a call of a channel
  rpc, is **K5511**; `Channel(sending:closed:receiving:)` outside the body of a service's channel rpc
  handler (its lambdas included, a private helper not) is **K5512**. A `@Load` initializer that calls a
  channel rpc, or an app function that does, through any chain of app functions (resolved calls, after
  the body pass), is the warning **K5513**, except in a client-rendered app or component. The receiving
  lambda is part of the handler's body, so `session.user` is narrowed there as in the handler. Handler
  shape needs nothing new: the handler repeats `Channel<C, S, E>` (K5505), `sending:` is a `Stream<S>`
  and the trailing lambda a `(C) async throws E -> Void` (K2001, K2203). A page or endpoint at
  `/_k/channels` is K5001 (`routing.rs`).
- **`RemoteClient.at(url:token:timeout:)`** (`k.rpc`, server-only) returns a client of its generic result
  `P`, written out (`RemoteClient.at<PaymentEvents>(…)`) or the type it is used as. A result that
  resolves to anything but a protocol is **K4026**; one that can't be inferred is K2002
  (`crates/k_sema/src/body.rs`, `check_remote_client`).

### 12.9 Async data: `@Load`

**v0**, design [14-async-data.md](../design/14-async-data.md).

- `@Load let x: T = await expr` initializers form an **ordering graph** by the `@Load` fields they read;
  independent loads run in parallel, dependent ones in order. A cycle → **K3020**.
- `@Load` values must be `Codable` (**K3021**) because server-rendered data is serialized to the
  browser.
- A declaration with `@Load` fields needs a `loading` block (**K3022**). `failed(let error)` binds the
  loads' error type: all loads of a declaration must throw the same type, because errors are wrapped,
  not unioned (K2001 at the second load's initializer otherwise). A nested `metadata { }` inside `failed`
  is allowed (it is lowered from the statement form, [22-metadata.md](../design/22-metadata.md)); the
  page-level `metadata` block is not evaluated when `failed` renders.

### 12.10 Forms and validation

**v0**, design [15-forms-and-validation.md](../design/15-forms-and-validation.md).

- `@Form var f: S = .empty` or `.empty(field: value, …)`: `.empty` is a generated static member of every
  `Validatable` struct with optional named arguments for any subset of its fields, typed as the fields,
  valid **only** as a `@Form` initializer (**K7002**).
- **`form.value` is readable only inside the action passed to `form.handle(...)`** (**K7001**). The
  checker tracks the function or lambda passed to `handle` and allows `value` in it and in the functions
  reachable from it whose every reference comes from it or another such function (a recursive helper
  qualifies; see Round 2 notes); anywhere else is an error.
- Constraint annotations (`@NotBlank`, `@Length`, `@Range`, `@Email`, `@Matches`, `@Check`, `@Format`)
  check their field types (K3002). `@Format(message:)` is valid on fields of parsed types and an error on
  `String` (**K7003**). `@Email` on an `EmailAddress` field → **K7004** (warning, removal fix).
- **`@Matches` takes a string literal** (**K7005**); the compiler parses the regex (ECMAScript syntax,
  the same on server and client) and an invalid pattern → **K7006**. It is allowed on `Secret<T>`
  config fields, where it checks the revealed value.

### 12.11 Config and secrets

**v0**, design [25-config-and-secrets.md](../design/25-config-and-secrets.md). A `config` declaration is
server-only data: its fields are `@Env("NAME")` values of `ConfigValue` types or `Secret<T>`, optionally
constrained (§12.10). Configs inject nothing (K4005) and are injectable into server-side services and
repositories. Any use from client-reachable code → K4007. `Secret<T>` values have no `==`, string
conversion or interpolation (K2001). Startup validation runs at run time.

### 12.12 Persistence

Design [19-persistence.md](../design/19-persistence.md).

- **v0:** entity fields must have column-representable types (**K6002**); `@Id`, `@Index`, `@Unique`,
  `@Relation`, `@Version` check their targets (K3002); entities and repositories are server-only;
  queries must be awaited (K2204); repository writes are only in repositories. Calls SQL can't express
  in a query lambda are **K6001**; `k_codegen` lowers the rest to `k_db`'s query IR
  ([db.md §4](db.md#4-the-query-ir-and-lowering)).
- **Migrations and `schema.lock`** are checked by `k_codegen::project_diagnostics` (§12.16), which
  replays the migrations with `k_db::builder::Replay` and compares the result with the lock and the
  entities: a migration that doesn't replay (**K6004**, at the call or the step of `alterTable`), a
  destructive one without `@Breaking` (**K6005**), a drop or rename of something live without
  `@Breaking("single instance")` (**K6006**), and the lock checks K8004–K8006.

### 12.13 Jobs

**v0**, design [30-background-jobs.md](../design/30-background-jobs.md). A service implementing `Job` is
server-only; its payload type is `Codable` (K5503's rule applied to jobs, reported as K2009 on the
payload); `@JobName`, `@Retry`, `@Schedule` check their arguments; the job's dependency graph can't
reach `Session`/`SessionManager` (**K4009**). The job's payload is compared with `storage.lock` (§12.16).

### 12.14 Tests

**v0**, design [21-testing.md](../design/21-testing.md). `expect cond` takes a `Bool` (**K7501**);
`expect throws E { … }` requires the block to be able to throw `E` (**K7502**), and the pattern may be
nested (`WeatherError.upstream(.serviceUnavailable)`). Test bodies may `use` fakes and config values
(§12.6); test files see `k.test` and test-only modules (§6). `TestClock` starts at
2026-01-01T12:00:00Z (a runtime fact the checker doesn't need).

#### Previews

Design [57-previews.md](../design/57-previews.md); `preview.rs`. A production file's `preview`
declarations go to its **preview scope** (`FileInfo::scope_of`): a second `FileInfo` over the same file id,
text, tree and imports that is a test file of the file's module. Its declarations are the previews, so
their bodies resolve as a `.test.k` file's (test-file declarations, `k.test`, test-only modules and
their public extensions) and nothing else in the file sees them; a private declaration of the file is
visible to them (`Program::same_file`). The scope isn't in `file_by_id` or its module's files: the
file-level rules, the import checks and the unused-import pass are its file's, and its previews are
checked, recorded and replayed with its file's body facts (`check_file_facts`). A preview's declaration
has an internal name no source can spell (`preview#Stem#offset`), so the resolutions of its `@Inject`
fields name an owner, and it is in no name table.

- Its `@Inject` fields are members, checked and injected as a test group's (`K::Preview` sits beside
  `K::TestSuite` in the layering, annotation-target and `use`-rule checks, K7503 included); its `use`
  rules are lines of its body.
- **Parameters** are the preview's signature (resolved before the bodies, like a function's): each needs a
  default (**K7504**) and a type the panel has a control for (**K7505**): `Bool`, `Int`, `Double`,
  `String`, an enum whose cases have no payload, `Money`, `Duration`, or a type with
  `static func parse(_ text: String) -> T?` (`Date`, `Url`, `Uuid`, …). In the body they are bindable
  locals, so `bind value` passes a `@Binding`.
- **The body** is checked as a test's (async, may throw, `try` without a `throws`), its last line as view
  content when it constructs a component, layout or page (`Checker::check_preview`: props, bindings,
  slots and modifiers are checked as in a `view` block), else as an expression: one that isn't a `View` is
  **K7508**, an empty body or one ending in another statement **K7509**.
- **Per file**: at most one unnamed preview (**K7506**), no two with the same name (**K7507**).
- **`testing` imports**: a production file with previews may import a test-only module; the import isn't
  K1104 and isn't an import-cycle edge, but each use of the name outside the file's previews is K1104 at
  the use (`check::preview_imports`). Names a preview uses don't count as the file's client references
  (K4007).
- **Feature gate**: each preview is K8108 at its keyword without `[package] preview = ["view-previews"]`
  (`stability::language_uses`); loose files may use it.
- `Analysis::previews(file)` lists the previews for the tools ([codegen.md §21.3](codegen.md#213-the-preview-bundle-entrypreviews)).

### 12.15 Time

`Date.now`, `.ago`, `.fromNow` and `Timer` don't exist (K1203 / K1201 like any unknown name). `Clock`
is injectable everywhere a service is (§12.6). Plain functions take `now: Date`.

### 12.16 Lockfiles

Design [13-tooling.md](../design/13-tooling.md#lockfiles). The host passes the project's lockfiles (and
the `k.toml` of the project and of each mounted library) to the semantic pass beside the Kobra sources.
`storage.lock` and `rpc.lock` are checked here (`locks.rs`, `rpclock.rs`), so the language server
reports them; `schema.lock` and `routes.lock` need code generation's persistence model and route table,
so `k_codegen::project_diagnostics` checks them, and `k check` (and the corpus) add its diagnostics for
an app (a project with `App.k`) without other errors. The language server doesn't run those.

**`storage.lock`** (design [30-background-jobs.md](../design/30-background-jobs.md#stored-payloads) and
[36-local-storage.md](../design/36-local-storage.md#file-format)) must be TOML of the documented
structure (**K8002** at the offending line). Each recorded `[jobs.<name>]` is matched with the program's
job of that name (`@JobName`, else the job whose qualified name the entry's `declaration` records), and
its recorded payload shape (`[types."…"]`) is compared with the payload type **by shape, not by name**: a
removed or renamed field, a changed field type, a new required field, a removed enum case or an optional
made required is **K8003** at the payload type argument of `implements Job<…>`, one note per change,
unless the job service is marked `@Breaking("reason")` (a job service accepts `@Breaking`). An optional
field, a new enum case and a required field made optional are compatible. Each recorded `[keys.<name>]`
is matched with the program's `StorageKey<T>` constant or field whose `StorageKey(name: "…")` literal is
that name, else with the one whose qualified name the entry's `declaration` records (then the key was
renamed, which is a change too), and its value type `T` is compared with the recorded `type` by the same
rules: **K8008** at the key's name, unless the constant or field is marked `@Breaking("reason")` (a
storage key accepts `@Breaking`). Each recorded `[collections.<name>]` is matched the same way with the
program's `Collection<Key, Value>` constant or field: a value shape that changed by these rules, or a
renamed collection, is **K8009** at its name unless it is marked `@Breaking("reason")`, and a different
`key` field is K8009 even then (`locks.rs`, `collections`). Each recorded `[keyNames.<name>]` is matched
the same way with the program's `KeyName<K>` constant or field (`k.crypto`, a key kept in a `KeyStore`):
a renamed key name is **K8010** at its name unless it is marked `@Breaking("reason")` (a key name accepts
`@Breaking`), and a different key type `K` is K8010 even then, since a stored key keeps the type it was
written with (`locks.rs`, `key_names`). New jobs, keys, collections and key names, and recorded
ones that no longer exist, are not reported. The pass also returns the `storage.lock` the program implies
(`locks.rs`, `implied`; `Analysis::storage_lock`), which `k build` writes: the namespace (recorded, else
the project's name), every key, collection, key name and job of the app's non-test code and the shapes of the app
types they hold, with entries the code no longer declares dropped; `None` for a library, for an app with
nothing to record and no lock, for an unreadable lock, and when the text is the lock's already
([52-indexed-collections.md](../design/52-indexed-collections.md#storagelock)).

**`schema.lock`** (`k_codegen`, `lockfiles.rs`; [db.md §11](db.md#11-schemalock)). In an app with
entities, migrations or a lock: an unreadable lock is **K8002**; the migrations are replayed in the lock's
order and every problem of the replay is reported (K6004–K6006, §12.12); a listed migration without its
file, a file whose `NNNN` isn't its position in the list, or a file the list doesn't name is **K8006**;
when the replay succeeds, every table and column it leaves must be what the lock records, ignoring entity
and field names (**K8005** per table at the lock's line, one note per difference; migrations without a
lock are one K8005); and the entities must match the lock (**K8004**, one per difference, at the field or
the entity, or at the lock's table for a removed entity), with the help naming `k migrate new`, or the
`k migrate rename` that records a rename when exactly one recorded field (or entity) is gone and one new
one of the same type appeared. `k_codegen::schema_lock` is what `k build` writes.

**`routes.lock`** (`k_codegen`, `routelock.rs`; design
[12-routing.md](../design/12-routing.md#routeslock-and-redirects)): TOML with `[pages.<case>]`,
`[endpoints.<case>]` (each `declaration` and `url`) and `[redirects]` (old URL template to `Page` case);
unreadable is **K8002**. A recorded page whose declaration and case are both gone is renamed when exactly
one unrecorded page of its module has the same path parameters (and matches no other entry). **K8007**: a
recorded URL nothing serves and no redirect keeps (at the lock's entry; at the page, with a label at the
entry, when the page still exists with another URL); a detected rename whose redirect isn't recorded (at
the new page, "run `k build`"); a redirect to a case that is no longer a page; a redirect whose old URL
lacks a path parameter the page needs, or has one the page doesn't. `k_codegen::routes_lock` is what
`k build` writes: the current pages and endpoints, the redirects (retargeted after renames, without those
a page now serves), one per detected rename, and the entries of removed URLs, kept until resolved. The
server's redirect table and `k routes` use the same redirects, so a rename is redirected before the lock
is rewritten.

**`rpc.lock`** (**v0**, `rpclock.rs`, design
[40-workspaces-and-services.md](../design/40-workspaces-and-services.md#compatibility-between-deploys)).
For an app, the protocols it *serves* are the non-stdlib protocols its own non-test services implement.
The lock is TOML:

```toml
[protocols."pay.Payments"]
service = "pay_contracts.pay.Payments"

[protocols."pay.Payments".rpcs.charge]
params = [{ name = "request", type = "pay.ChargeRequest" }]
returns = "pay.Charge"
throws = "pay.ChargeError"

[types."pay.ChargeRequest"]
fields = [
    { name = "orderId", type = "String", number = 1 },
    { name = "cents", type = "Int", number = 2 },
]
reserved = [3]

[types."pay.ChargeError"]
cases = [
    { name = "declined", fields = [{ name = "reason", type = "String?" }] },
    { name = "rpcFailure", fields = [{ name = "_0", type = "RpcFailure" }] },
]
```

Types are written like `storage.lock`'s (stdlib types by simple name); `[types]` holds every app or
library struct and enum reachable from the served signatures. An enum's `cases` are inline tables, each
with its `name` and, when it has a payload, its `fields` (`name` and `type`; an unlabelled payload is
named `_0`, `_1`… by position, as the wire names it). Payload fields have no numbers: a case's payload is
JSON on the wire today, and numbers come with Protobuf enums. **A case written as a bare string** (`cases =
["declined", "rpcFailure"]`) is how locks were written before payloads were recorded; it still reads, and
its fields, being unknown, aren't compared (the payload *types* are still followed through the program, as
before). There is no format key and none was added: the new form is a superset the reader tells apart per
case, and `k build` rewrites an old lock in the new form the next time it writes it. Malformed TOML, a field number that isn't a
positive integer, a number used twice in one type or both used and reserved → **K8002** at the line. The
comparison, per recorded protocol: the wire `service` changed (the Protobuf package, from the declaring
package's name and the module, and the protocol's name; a lock without `service` isn't compared); an
rpc, parameter, field or case removed; a parameter, field, result or error type changed (by written
type); a new required parameter or field (not optional, no default); and for a case recorded with its fields, the
same three changes to one of its payload fields →
**K8001** at the protocol's name, one note per change, a label at the protocol's line in the lock, unless
the protocol is `@Breaking("reason")`. A recorded protocol the app no longer serves → K8001 at the app's
name. New rpcs, optional fields and parameters and new cases are compatible. **Writing:**
`Analysis::rpc_lock()` is the lock the program implies: recorded field numbers kept, new fields numbered
after every number used or reserved, removed fields' numbers added to `reserved`; `None` for a library or
an app that serves nothing and has no lock. `k build` writes it when it differs; `k check` never writes.
An unreadable lock is reported and the implied lock is the program's alone.

**Channel rpcs** in `rpc.lock` ([44-channels.md](../design/44-channels.md#rpclock-and-compatibility))
are written with `kind = "channel"` and the three types of `Channel<C, S, E>` in place of `returns`:

```toml
[protocols."chat.ChatProtocol".rpcs.room]
kind = "channel"
params = [{ name = "id", type = "Int" }]
send = "chat.ChatCommand"
receive = "chat.ChatEvent"
refusal = "chat.CommandError"
throws = "chat.RoomError"
```

Other rpcs have no `kind` (unary and stream rpcs differ by `returns`, `Stream<T>` or not), so locks written
before channels read unchanged; a `kind` other than `"channel"`, or a channel without `send`, `receive` or
`refusal`, is K8002. Comparing: an rpc that was a channel and isn't, or the reverse, and a changed `send`,
`receive` or `refusal`, are K8001 notes. **The direction matters for one rule only:** a type reached in
the program *only* through channels' `receive` types (the server's messages, which an old tab skips when
it doesn't know a case) may lose an enum case; a type also reached from a parameter, a result, an error,
a `send` or a `refusal`, of any served protocol, keeps every case ("the stricter rule of each place it
appears"). Added required fields, removed fields and changed types break both directions, for a struct's
fields and a case's payload fields alike: a `S` case gaining a required field can't be decoded by an open
tab, and a `C` case gaining one is never sent by it. A removed case's fields aren't compared. The
comparison follows enum case payloads into the types they carry, so a struct carried only inside an event
enum is compared field by field.

`Analysis::file_dependencies(file)` lists the files whose declarations a file's names, types and members
resolve to; `k test --watch` uses it to find the tests a change affects.

### 12.17 Stability

`crate::stability` runs last, after every body is checked, over the revision's resolutions (the index the
tooling queries read, so replayed files count like checked ones) and the declarations' annotation lists
(annotations are known by name, not resolved): a use of a `@Deprecated` declaration or member is K1401,
a use of a `@Preview("id")` stdlib declaration in a package whose `k.toml` doesn't list `id` is K8108,
a mounted library's `preview` id the project doesn't list is K8109, and the project's own list is
checked against `k_sema::features` (K8101 for unknown or planned ids, K8110 for stable ones). Imports,
doc links, the deprecated declaration's own body and the stdlib's files aren't uses. `@Internal` is
applied earlier, by the prelude and import resolution (K1102). Decisions:
[48-releases-and-versioning.md](../design/48-releases-and-versioning.md#stability-levels-decided-while-implementing-2026-11-12).

---

## 13. Diagnostic codes

The registry is `crates/k_diagnostics/src/codes.rs`; every code's primary span and fix is in
[diagnostics.md](diagnostics.md#semantic-codes). Sub-ranges per area:

| Range | Area | Section |
|---|---|---|
| K1000–K1099 | project layout and files | §4 |
| K1100–K1199 | imports and the module graph | §6 |
| K1200–K1299 | name resolution | §7 |
| K1300–K1399 | visibility | §6 |
| K1900–K1999 | doc comments | §7.5 |
| K2000–K2099 | types and expressions | §9 |
| K2100–K2199 | units and number extensions | §9.4 |
| K2200–K2299 | effects | §10 |
| K2300–K2399 | control flow | §11 |
| K2400–K2499 | stdlib-specific type rules | §9.7 |
| K3000–K3099 | annotations, members, reactivity, `@Load` | §12.1–12.3, §12.9 |
| K3100–K3199 | views | §12.4 |
| K3200–K3299 | styles | §12.5 |
| K3300–K3399 | themes | §12.5 |
| K4000–K4999 | services, DI, layering, server/client boundary, `use` | §12.6 |
| K5000–K5499 | pages and routing | §12.7 |
| K5500–K5999 | protocols and RPC | §12.8 |
| K6000–K6999 | persistence | §12.12 |
| K7000–K7499 | forms and validation | §12.10 |
| K7500–K7999 | tests | §12.14 |
| K8000–K8099 | lockfiles | §12.16 |
| K8100–K8199 | manifest and packages | driver |

Codes for **v0** checks are registered now so test-first corpus cases can name them. A **deferred**
check gets its code when it is implemented, in its area's range.

---

## 14. Grammar implications checklist

Every item collected while adopting the design (grammar implications and semantic-only rules) is either
planned here or explicitly deferred:

| Item | Status |
|---|---|
| `config` and `migration` declarations | parsed (stage 1); §12.11, §4 (K1005) |
| `use ( Type \| Expr ) for Type` | parsed (RA-35); §12.6 (K4011, K4010) |
| `for await` | parsed; §11.2 (K2310, K2311), §10 (K2205) |
| Top-level theme overrides; re-declared library tokens | parsed; §12.5 (K3301, K3302) |
| `variant … matches .condition` | parsed; §12.5 (`MediaCondition`) |
| Nested `metadata { }` inside `failed` | §12.9 (lowered from the statement form) |
| No keyword member names (`theme.apply`, not `use`) | parser rule; stdlib uses `Theme.apply(_:)` (§5.2) |
| `rpcFailure(RpcFailure)` spelling; rpc `throws` required; `RpcOnlyError` | §12.8 (K5501, K5502) |
| `@Inject` always private, `public` on it an error; `public` on members a warning | §6 (K1303, K1302) |
| Top-level `case _` → `default`; unused `case let x` / `catch let x` | §11.1 (K2303, K2304) |
| `@Effect` and event handlers can't throw | §10 (K3012) |
| A query returned or passed without `await` | §10 (K2204) |
| Comparing a `Bool` with a literal is a lint | §9.6 (K2012) |
| Endpoint pages need `at`; no `requires Authenticated`/`Role` on them | §12.7 (K5004, K5005) |
| Error-generic parameters `() async throws E -> T` | §10 |
| `return` inside a lambda | §11.2 (K2308) |
| `@Form` `.empty` / `.empty(field:)`; `form.value` only in `handle` | §12.10 (K7002, K7001) |
| Memberwise initializers: defaults may be omitted, others not | §9.2 (K2008) |
| `@Format(message:)`; `@Email` on `EmailAddress` redundant | §12.10 (K7003, K7004) |
| `@Matches` literal regex, parsed; allowed on `Secret<T>` | §12.10 (K7005, K7006) |
| `HttpStatus` enum with `.other(Int)`; nested status patterns | §9.7 (K2401) |
| Time: no `Date.now`/`.ago`/`.fromNow`/`Timer`; `Clock` injectable in every scope, not a platform interface; `TestClock` implicit; discarded `TimerHandle` | §12.15, §12.6, §12.3 (K3013) |
| `clock.now` not reactive | §12.3 |
| Test group names are free; several groups per file | §4 |
| Doc-comment links | §7.5 (K1901) |
| An assignment member outside a theme | §12.1 (K3303) |
| Keyframes: selectors, content, references, scoping | §12.5 (K3205–K3208) |
| No element selectors (RA-8) | parser (K0116); §12.5 |
| Unused `failed(let error)` binding | §11.1 (K2304) |
| Whether a non-throwing function may be passed where a throwing one is expected | decided here: yes (§9.6, function subtyping) |
| Whether a `@Check` function may throw | deferred to the forms phase; v0 requires `async -> Bool` (K2001) |
| Whether a complete struct value is a valid `@Form` initializer | v0: no, only `.empty(...)` (K7002 names the fix) |
| Theme cookie in client-rendered apps; `render(...)` argument reactivity | runtime, not checked |
| `routes.lock`, `schema.lock` checks | in `k_codegen` (`project_diagnostics`), which has the route table and the persistence model; `k check` runs them for an app without other errors (§12.16) |
| `protocol.lock` | folded into `rpc.lock` (§12.16) |

---

## 15. Performance

Budgets from [architecture.md §12](architecture.md#12-performance-budgets): a cold check of 100k lines
in ≤ 1.0 s on 8 cores, a warm body edit in ≤ 30 ms.

- Parsing and per-declaration checking fan out over all cores (§3.5); module-level layers are linear in
  the number of declarations.
- Names and types are interned; bodies are arena-allocated and dropped as a unit; item trees and def
  maps use `u32` ids, not strings.
- No quadratic algorithms: def maps are hash maps; import cycles use Tarjan's SCC; exhaustiveness is the
  usefulness algorithm, linear in practice; the DI graph and `@Load` graphs are DFS with memoization;
  the CSS value matcher is a backtracking-free recursive descent over the value grammar with memoized
  `||` / `&&` alternatives.
- The stdlib is parsed once per process and its item trees, def maps and signatures are computed once and
  shared by every host (they never change).
- Early cutoff (§3.3) keeps warm edits to one declaration's layers 6–9. **Implemented at file level
  (round 2):** each file's body facts are kept and replayed while its text and every project signature
  are unchanged; signatures are resolved eagerly before the bodies so their facts never depend on which
  body needed them first. A file's signature text is its tokens minus trivia and minus function,
  method, initializer and rpc bodies; facts that mention another file's positions (a label, an edit)
  are replayed only while that file is unchanged too. A **project pass that reads no body** (today the
  `rpc.lock` check, §12.16) is replayed the same way (`warm::PassFacts`): its diagnostics and its value
  are kept and reused while the file list, every signature, and the lockfiles' and manifests' texts are
  unchanged and every position its diagnostics mention is in an unchanged file. Function `where`
  clauses are resolved eagerly with the other signatures, so the program view resolves nothing the
  result lacks.
- `benches/sema.rs` benchmarks the full pipeline over generated projects (cold, warm after a body or a
  signature edit, pathological families), and `tests/perf_budget.rs` enforces the budgets through
  `scripts/bench-check.sh`; regressions over 5% need a stated reason (testing.md §9). Baselines,
  profiles and the budget status (all met since round 2) are in
  [performance.md §10](performance.md#10-semantic-analysis).
- Exhaustiveness stays linear in the number of arms for enums with thousands of cases: a constructor's
  fields are computed from that constructor alone, a wildcard column groups rows by head constructor
  once, and `pattern::Rows` keeps a growing matrix as a trie indexed by head pattern column after
  column (constructors and literals), so each arm is tested only against the rows that can overlap it,
  also when thousands of arms share a constructor (`case .c(1)`, `case .c(2)`, …: 20k such arms took
  12.7 s before round 2, 0.04 s after). A node with wildcard-headed rows falls back to the flat
  algorithm over its rows. The missing patterns of K2301/K2206 test each constructor once against the
  trie.
- Unification and assignability resolve bound variables only at the top of each level (deep
  resolution at every level was cubic in generic nesting depth); `Checker::resolve` allocates only
  when a type actually contains a bound variable.
- Recorded types (`type_at`) are stored as `Ty` and rendered on demand from a per-revision name table,
  not rendered eagerly for every expression. The checker's memo tables use an Fx hasher (`fx.rs`).

---

## 16. Implementation plan

### Round 1a notes

How round 1a implements §3–§11, and where it deviates from the target architecture (each deviation is
an open item for a later round):

- **Pipeline.** `check::analyze` runs once per revision, memoized in the host's state: `program`
  collects files, modules, declarations and members from the parses (the item tree of §7.1, kept as
  syntax nodes); `cx` resolves imports, names, types, signatures and conformance on demand, memoized in
  per-analysis tables, so every signature error is reported once; `body` checks each body; `pattern`
  is the usefulness algorithm; `doclinks` checks `///` links. The pass is **single-threaded** and runs
  per revision, but since round 2 it replays the **body facts** of every file whose text didn't change
  when no signature in the project changed (`warm.rs`; a file-level early cutoff, §3.3): a cold check of
  the generated 100k-line benchmark project takes about 0.1 s in a release build, a warm body edit about
  15 ms (performance.md §10). Per-declaration parallelism and cutoff below the file level (§3.3, §3.5)
  need `Send` syntax pointers instead of rowan nodes.
- **No HIR yet.** Bodies are checked directly over the typed syntax tree (§8 describes the target).
  Types are immutable structural values (`ty::Ty`, generic arguments shared through `Arc`); type
  equality is structural equality, not interned ids.
- **Lenient framework surfaces** (round 1a only; round 1b checks views, styles, component calls,
  repository bodies, `Endpoint` and the session shells, see "Round 1b notes"). Still lenient: `Classes`
  accepts any value outside `.class(…)`, and a missing static member of a service, protocol or entity
  (`Job.kind`, `Product.UniqueViolation`) is not reported.
- **Decided while implementing** (recorded here as the source of truth):
  - Enums without payloads are `Equatable` and `Hashable` without declaring it (they compare by
    case); structs and enums with payloads opt in with `implements` (§9.2).
  - Components, pages and layouts are values of `View`.
  - A `@Form var f: S` field reads as `Form<S>` (`f.value`, `f.handle(…)`); its initializer is
    checked against `S`, where `.empty` and `.empty(…)` are accepted for `Validatable` structs.
  - `session.user` (`k.auth.Session`) is `SessionUser`, not optional, only where the server has
    already run an `Authenticated` guard: every body of a page whose `requires` clause names
    `Authenticated`, and the handler of an rpc whose protocol, or whose rpc in that protocol, does.
    One handler serves the rpc of every implemented protocol that declares it, so each of them must
    guard it; one that leaves it open lets guests run the body. Other guards (`RateLimit`, `Role`, `ServiceAuthenticated`), a `requires` written on the
    implementation's rpc (the server reads the protocol's) and private helpers of a guarded service
    leave it `SessionUser?` (`Checker::guarded`).
  - A `@Builder` struct's trailing block is its content, and its fields with defaults are chained
    setters (`HttpRequest(method: .get, url: u).query(q)`).
  - Functions of one type may be overloaded by labels and parameter types (`Hmac.sha256(key:)` with
    `Bytes` or `Secret<String>`); a call picks the first overload whose labels and argument types fit,
    preferring one whose last parameter is a function when a trailing lambda is given. Every label
    counts, a lambda argument's too, and a labelled lambda needs a function parameter:
    `contains(where: { … })` is `contains(where:)`, never `contains(_:)`.
  - A property and a method may share a name (`HttpResponse.body` and `body(_:)`): a call prefers the
    method, any other use the property.
  - The stdlib's mutating methods are `append`, `insert`, `remove`, `removeAll`, `removeWhere` and
    `set` (`cx::MUTATING_METHODS`); calling one through a `let` is K2011 at the receiver.
  - Operators beyond §9.4: `Date ± Duration` is a `Date`, `Date - Date` a `Duration`, and `+`
    concatenates `String`s, `List`s and `Bytes`.
  - Optional chains propagate: in `a?.b.c` the `.c` reads the unwrapped `b`, and the result is
    optional; `?.` and `.` on a non-optional and `??` on a non-optional are K2018.
  - An unresolved callee's arguments are still resolved (K1201 inside them), but implicit members
    among them report nothing (no K2007). When a call has an unknown label (K2004), missing arguments
    aren't also reported.
  - A member of a non-`public` extension used in another module is K1301 (out of reach, §6), like
    any declaration beyond its visibility.
  - Test files see `k.test` without an import (§4). `k.ui.Router` is in the prelude (the design uses
    `Router.navigate(to:)` without an import everywhere).

### Round 1b notes

How round 1b implements §12, and the decisions it took (each is the source of truth until a later
round revises it):

- **Modules.** `framework.rs` holds the declaration-level rules (annotations, blocks, themes, forms
  constraints, `@Load`, entities) and the shared state (style classes and keyframes, references,
  `form.value` actions, table types); `body/view.rs`, `body/style.rs` and `body/annotate.rs` extend the
  body checker with view content, style blocks and annotation arguments; `di.rs` is the DI graph,
  layering, scopes and sides; `routing.rs` the route table and guards; `rpc.rs` the protocol rules;
  `app.rs` configs and test groups; `regex.rs` the `@Matches` validator. The project-level passes run
  once, after every body, in that order: routing, rpc, app, DI, client references, unused imports,
  unused classes/keyframes/hooks.
- **Views.** View content is checked in *view mode*: each expression statement is a node whose base
  decides what it is. A lowercase name that isn't a local, member or declaration is an element (an
  element name followed by a trailing block wins over a member of the same name); a name resolving to a
  component, page or layout is a component call; anything else (text, a slot, a scoped slot call) is a
  value. `item.name` with a bare member access is always a value. Handler lambdas leave view mode;
  trailing blocks and slot builders enter it. `Content::Only` lists restrict element and text
  children; component calls and opaque `View` values are allowed there, since they may render the
  listed elements. `metadata { … }` inside view content is the statement form of a metadata block.
- **Bindings.** A text input (`Binding<String>`) binds any field of a `@Form` struct through the form's
  text draft (`bind signup.email` for an `EmailAddress`); other bindings need the place's exact type.
  Loop variables over a bindable list are bindable. A `Void` parameter takes no argument
  (`closed.emit()`).
- **Handlers.** A function passed to `.on(…)` that throws is K3017; a lambda whose body lets an error
  escape is K2203 (its expected type throws nothing). Handlers aren't server-rendered code.
- **Annotations** resolve by name: one the file doesn't import resolves to the annotation table's entry
  of that simple name when exactly one exists (the checker already treated annotations by name);
  annotations the table doesn't know are not reported. `@Slot` also applies to layout fields
  (`Target::LayoutField`, the page's content).
- **Styles.** Values are matched against the property's grammar by computing every reachable end
  position; when none consumes the whole value, K3202 is at the first component past the furthest
  match, or the whole value when it is merely incomplete. The CSS-wide keywords (`inherit`,
  `initial`, `unset`, `revert`) are accepted alone; `<color>` accepts the basic named colors,
  `transparent` and `currentColor`; `0` is a length. An implicit member resolves against the first of
  the property's types that has it; a token of another category is K3202 (the value is wrong), an
  unknown name K2006. `{ }` must read a reactive place (a field with a reactive annotation, a path into
  one, or a service's state), otherwise K3210, with the brace-removing fix for tokens and literals.
  Unused classes (K3204) and keyframes (K3206) are reported after every body.
- **Themes.** Tokens are unique per category (`space md` and `radius md` don't clash); a library token
  is one a stdlib theme declares; overrides resolve against the theme's own tokens, then every theme's.
- **Reactivity and ownership.** K3014/K3010 replace K2011 for `@Prop`/`@Derived` places; a wrongly
  declared `@Prop var` is reported once (K3001). K4006 is reported at any write to a service's field
  from outside the service, through any path or mutating call. The stdlib mutating methods count as
  writes only on `List`, `Map` and `Set`.
- **DI.** `@Inject` types are classified as services, interfaces (resolved through `use` rules or a
  single implementation among services outside test code), protocols, repositories, configs, platform
  interfaces (stdlib `@Platform`) or server infrastructure (`Database`, `SessionManager`). Endpoint pages
  inject anything but protocols and repositories; tests inject anything injectable. Scope lifetimes rank
  `.server` > `.app` > `.request` > `.route` > `.component`; `Clock` is exempt; an unknown scope counts
  as `.app`. Cycles are every `@Inject` edge inside a strongly connected component.
- **Sides.** As §12.6, plus the stdlib's `@ServerOnly`/`@ClientOnly`/`@Platform(side:)` markers
  (`Secret<T>` is `@ServerOnly`). K4007 for a server-only declaration imported into a file that holds
  only client-reaching code is reported once, at the import; other references are reported where they
  are (a generic type whole), never where another diagnostic already is, and a layering violation
  (K4005) takes precedence. K4008 is reported only at the service whose own injections join the sides;
  a protocol implementation that needs the browser is K5504 instead. K4012 covers the stdlib's
  client-only APIs called from field initializers and views of projects that render on the server
  (`init()` and handlers run in the browser). K4013 covers user services only (`Theme` carries what the
  server needs).
- **Routing.** A page's default URL is `/` plus its kebab-case name plus `/{param}` per path
  `@Param` (no default, not optional); URLs conflict when their shapes (parameters as `{}`) are equal,
  reported at the later page by file path, then offset. Endpoint pages are `Endpoint` cases, not
  `Page` cases. A page case's payload is the page's `@Param`s; optional ones may be passed by name or
  chained on a `Page` value (`.product(id: 1).tab(.reviews)`). Guards are types marked `@Guard`, their
  arguments checked like a call. `SessionUserId` and `SessionRole` stand for the app's one `AuthUserId`
  struct and `AuthRole` enum; without one they are closed, empty shells (so `Role(.admin)` is K2006).
- **Protocols.** Handler signatures are compared without `async` (handlers are async, rpcs aren't
  declared so); an error enum need not declare `Codable`, but its payloads must be.
- **`@Load`.** An initializer's error is the first error that escapes it; `failed(let error)` binds the
  declaration's first load error. Load errors must implement `Codable` (K3024). `reload` is a
  generated `() -> Void` member. A call `chart()` next to `@Load let chart` resolves to the declaration
  in scope, not the field.
- **Forms.** `.empty` and `.empty(field: …)` (every field optional) are valid only in a `@Form`
  initializer. `form.value` is readable in the actions passed by name to `form.handle(…)`, lambdas passed
  to it, and functions every reference to which comes from such a function. In a `@Matches` literal an
  interpolated integer literal (`"[A-Z]{3}"`) is regex text; any other interpolation is K7005. Patterns
  are validated against the ECMAScript grammar with Annex B's leniency for a `{` that starts no
  quantifier.
- **Configs.** `ConfigValue` types, enums without payloads and lists of either are config values; a
  field whose annotation is K3002 isn't also K4017. Constructing a config outside test code is K4020.
- **Persistence.** Inside a repository the entity's plural (`notes`, `categories`) is a
  `k.db.Query<Entity>`; awaiting a query yields a list; the collection also has the writes
  (`insert`, `insert(all:)`, `delete(_:)`); the row of an `update { row => … }` lambda is writable.
  Column types are the scalars, `Date`, `Uuid`, `EmailAddress`, `Url`, `Money`, `Duration`, enums without
  payloads, the app's `AuthUserId` struct, entities and lists of entities (relations), and optionals
  of these. **K6001 is checked for a v0 subset:** a call of a user function or method inside a lambda
  passed to a query method; the full translation to `k_db`'s query IR stays in stage 3.
- **Tests.** `expect throws E.case { }` needs a block that can throw `E` (K7502); `expect` takes a
  `Bool` (K7501). A fake (`service F implements S` in test code) provides every non-private function and
  field of `S`. A test group's `use` rule restating a stdlib `@TestDefault` is K4014.
- **Metadata.** Metadata statements call `k.ui.MetadataEntries`; `status(_:)` is valid only in a
  `metadata` block inside `failed` (K3025). After the bodies, `metadata.rs` walks the `metadata` blocks
  of pages, layouts and the app: a literal `description(…)` over 160 characters is the warning K3028,
  and, in a program with an `app`, a view page with no `title` from its own page-level block, the layout
  it renders in (its `@Layout`, else the app's, none with `@NoLayout`) or the app's is the warning K3027,
  with a maybe-incorrect fix that adds `title("…")` from the page's name.
- **Responses.** `HttpResponse.headers([...])` with a string-literal key `set-cookie` (in any case) is
  K5013 at the key; cookies are set only through `Session`. A computed name panics at run time.
- **Jobs.** A job service's generated `static let kind` is typed `JobKind<Payload>` from its
  `Job<Payload>`, so generic calls such as `payloads(of:)` infer the payload (and its codec).
- **Closest names** use the optimal-string-alignment distance (a transposition costs 1, so `titel`
  suggests `title`); HTML attribute spellings (`src`, `alt`, `href`) suggest their Kobra labels.
- **Remaining leniencies** (open items): a repository's members seen from outside it stay open (its inherited writes and generated queries
  aren't modelled, nor `Entity.UniqueViolation`); `@container`/`@layer` preludes, accessibility lints
  beyond required names and stream restrictions remain deferred; the pass is still single-threaded.

### Round 2 notes

Round 2 turned every pending regression case regular and settled the findings other agents reported.
Decisions (each is the source of truth until a later round revises it):

- **Fix placement.** An inserted `try `/`await ` goes before the *covering expression*: the outermost
  binary operand, prefix operand or postfix receiver the call starts, stopping at arguments, list
  elements, parentheses and statements (grammar.md §13.2, K0114). A `try` also covers an enclosing
  `await`. The K2201 fix is machine-applicable only when the error then has a handler (an enclosing
  `do`, a `throws` naming it, an inferred lambda error); otherwise it is maybe-incorrect, because the
  edit alone turns K2201 into K2203. K3001's `var`-to-`let` is maybe-incorrect when the declaration
  assigns the field.
- **Signature labels.** K2203 and K2205 label the enclosing function's name (or `init`) and offer
  ``Add `throws E` `` / ``Add `async` `` (maybe-incorrect), inserted after the parameter list (`throws`
  after `async` when present). Lambdas and field initializers have no signature: no label, no fix.
- **Closest names** never suggest the written name itself. K1201/K1202 offer the closest name in
  scope (locals, `self`'s members, the module, imports, the prelude) when no import fix applies.
- **Fix inventory added:** K2003/K2008 insert `label: <#Type#>` placeholders; K2010 inserts stubs
  copied from the requirement's signature; K2301/K2206 list the missing top-level patterns in a note
  (at most eight, then a count) and insert empty arms/clauses (`default`/`catch _` when only a
  wildcard covers the rest); K2302 removes the arm or clause; K1106 and the K5501/K5502 fixes import
  what they name; K4014 also removes the imports only the rule used. K1004, K1103 (the rest of the
  cycle, by shortest path) and K4003 (`… is part of a dependency cycle: A → B → C → A`, self-injection
  `A injects itself`) carry the labels diagnostics.md lists; K4007 at an injection notes the path to
  the server-only declaration and why it is server-only.
- **Typing.** Unary `-` passes an optional expected type's payload to its operand. A non-throwing
  function is assignable to `throws Err` without fixing an error variable (the call binds it to
  `Never` if nothing else does), so `twice(a: { 1 }, b: { try fe() })` infers `Err = E`. In
  `u?.address.city`, `?.` unwraps `u` only: a member whose own type is optional needs its own `?.`
  (K2017). The value of a block's last expression of type `Never` ends the path, as a statement does
  (definite initialization). The arguments of a call whose callee doesn't resolve are checked normally,
  so independent errors in them are reported at once. Calling an `rpc` is async whether or not the
  declaration says `async`.
- **New codes.** K2019 `duplicate-member` (fields, computed properties and cases in one namespace;
  functions may overload; the first member wins); K3026 `derived-cycle` (a rule added to
  03-reactivity.md); K5506 `handler-throws-rpc-failure` (28-rpc-wire-format.md: only the runtime
  produces `rpcFailure`).
- **Call-graph rules.** `form.value` is readable in the functions reachable from a `handle(…)` action
  every reference to which comes from the action or such a function, so recursion qualifies and a
  function only calling itself doesn't. K3011 is reported at a call, from a `@Derived` initializer, of
  a method of the same declaration that writes its fields directly or through its own calls (a
  syntactic summary per declaration). The `@Load` ordering graph has `@Derived` fields as intermediate
  nodes. **Code that reaches the browser** (K4007) is, transitively, components, layouts and pages with
  a view, the top-level functions they reference and the shared services they inject; references in
  `@Load` initializers aren't followed, because the server render awaits them and K3021 guards what
  crosses.
- **`Secret<T>` interpolation** is allowed and renders `Secret(•••)`, as 25-config-and-secrets.md
  says; §9.7 said otherwise and was corrected.
- **Cascades.** One K2002 for a nest of unannotated lambdas, at the outermost.
- **Warm re-checks** replay unchanged files' body facts (§15, `warm.rs`); `tests/warm.rs` compares
  warm and fresh analyses (diagnostics and every `type_at`/`resolve_at` answer) over crafted edits and
  edits to every example, and `host.rs` tests that unchanged files are replayed, not re-checked. They
  also compare the implied `rpc.lock` and the program view, and cover replayed `rpc.lock` facts (body,
  lock and manifest edits) and the page passes (K3027, K3028, K3204) after body edits.

Each step lands with its corpus cases turning from pending to regular.

1. **Item trees, module graph, def maps, imports, visibility, file rule** (K1001–K1306).
2. **HIR lowering and name resolution in bodies** (K1201–K1203), `resolve_at`.
3. **Types and inference** (§9), `type_at`; **effects** (§10); **control flow** (§11).
4. **Components, reactivity, views, styles, themes** (§12.1–12.5). Done in round 1b.
5. **DI graph, layering, boundary** (§12.6), `side_of`. Done in round 1b.
6. **Routing, protocols, `@Load`, forms, config, jobs, tests, doc links** (§12.7–12.14, §7.5). Done in
   rounds 1a (doc links), 1b and 2 (the job rules of §12.13: K4009 at the job's name with the path in a
   note; the payload's `Codable` rule is `Job`'s generic constraint, K2009 at the type argument).
7. Stage 3: persistence lowering, lockfiles, codegen.

## 17. Tooling queries

The language server (`crates/k_lsp`, [lsp.md](lsp.md)) answers every request from the same `Analysis`
snapshots as `k check`, so the editor never disagrees with the compiler (design 13-tooling.md). Besides
`type_at`, `type_info_at`, `resolve_at`, `side_of` and `program`, the snapshot answers:

| Query | Answer |
|---|---|
| `references(&Resolution)` | every span referring to a symbol, project-wide, sorted, without duplicates |
| `file_references(FileId)` | every resolved reference of a file, sorted by range (the reference under a cursor, document highlights) |
| `definition(&Resolution)` | where a declaration, member, local, parameter, theme token, style class or keyframes rule is declared; `None` for built-in elements, attributes, events and CSS properties |
| `implementations(DeclId)` | the declarations implementing an interface or protocol, transitively through other interfaces |
| `page_url(DeclId)` | a page's URL template: its `at` template, or `/kebab-name` plus its required `@Param`s |

**What is recorded.** Besides the references bodies and signatures record, two kinds of reference are
recorded for tooling: the **last segment of a bound import** (`Square` in `import geo.Square`, recorded in
the live chunk when imports are resolved), and **each segment of a resolved doc link** that names
something (``[`Shape.area`]`` records `Shape` and `area`; ``[`k.http.HttpStatus`]`` records
`HttpStatus`; a parameter link records a `Parameter`). Rename and find-references therefore cover imports
and documentation. Module segments of import paths are not symbols and record nothing.

**The reverse index is incremental.** The resolutions of a revision are chunked: one live chunk and one
body chunk per file, shared by pointer with the previous revision when the file's body facts were
replayed (§15, `warm.rs`). `refs.rs` indexes chunk by chunk (by target and by file) and keeps the index
paired with its chunk, so the next revision reuses the index of every chunk it shares. After a body edit
only the edited file's chunk and the live chunk are indexed again (`host.rs` tests it). The index is
built by the first tooling query of a revision, never by the check, so `k check` doesn't pay for it.

**Definitions are spelled like their references.** For every reference whose definition is known, the
text at the reference equals the text at the definition, with one exception: a page case of `Page` or
`Endpoint` (`.signIn`) is defined by its page (`SignIn`). `crates/k_sema/tests/tooling.rs` checks this
over every example app and over well-typed generated projects (a proptest); it is what makes rename a
plain text replacement at each reference.

**Known limits.** A test suite named like the declaration it tests shares its display name, and
`ProgramView::resolve` answers the first of the two (the tested declaration). A generated member other
than a page case (`copy(…)`) has no definition.
