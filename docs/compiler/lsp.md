# Language server

`k lsp` is the Kobra language server: the Language Server Protocol (LSP 3.17) over standard input and output,
answered by the compiler's own analysis. The design rule is [13-tooling.md](../design/13-tooling.md): **the
compiler and the language server are the same engine.** Every request is answered from an
`k_sema::Analysis` snapshot of the same `AnalysisHost` `k check` drives, so the editor never disagrees
with the compiler. The crate is `crates/k_lsp`; the editor integrations are in `editors/`
([§11](#11-editors)).

Contents:

1. [Dependencies](#1-dependencies)
2. [Architecture](#2-architecture)
3. [Protocol coverage](#3-protocol-coverage)
4. [Positions](#4-positions)
5. [Diagnostics and quick fixes](#5-diagnostics-and-quick-fixes)
6. [Navigation, references and rename](#6-navigation-references-and-rename)
7. [Hover](#7-hover)
8. [Completion and signature help](#8-completion-and-signature-help)
9. [Robustness](#9-robustness)
10. [Testing and budgets](#10-testing-and-budgets)
11. [Editors](#11-editors)
12. [Limits and next steps](#12-limits-and-next-steps)

---

## 1. Dependencies

| Crate | Use |
|---|---|
| `lsp-types` 0.95 | the protocol's request, notification and result types (serde); brings `url` for `file://` URIs |
| `serde`, `serde_json` | JSON-RPC bodies |
| `regex` (dev) | evaluates the TextMate grammar's patterns in `tests/editors.rs` and `tests/textmate.rs` |

**Why not `lsp-server` or `tower-lsp`.** `tower-lsp` is async (tokio) and wants `Send` futures per request;
the analysis is synchronous and already has its own cancellation (a snapshot answers `Cancelled` once a
newer revision exists), so an async layer would add a runtime and nothing else. `lsp-server` (rust-analyzer's
transport) fits the synchronous model, but its reader thread ends the connection on the first body that
isn't valid JSON, and it owns the thread and channel layout. The transport is about 350 lines
(`transport.rs`: framing, classification, error codes, with tests) and the loop about 1,000
(`server/mod.rs`, with tests), and owning
them is what lets the server answer a malformed message with a JSON-RPC error and carry on
([§9](#9-robustness)). `lsp-types` is used only for the protocol's data types.

## 2. Architecture

```text
stdin ─► reader thread ─► frames ─┐
                                  ▼
            main loop (owns the Workspace: one AnalysisHost per project)
              │  applies didOpen/didChange/didClose at once, in order
              │  takes a snapshot (Analysis) per request
              ▼
          worker pool (4) ─► results ─► main loop ─► stdout
```

| Module | Role |
|---|---|
| `transport` | `Content-Length` framing; `Frame::{Message, Malformed, Eof}`; JSON-RPC and LSP error codes |
| `server` | the message loop, the worker pool, debounced diagnostics, cancellation |
| `server::handlers` | protocol parameters in, protocol results out, on a `Snap` (snapshot + root + client capabilities + line indexes) |
| `workspace` | projects, files on disk, open documents, the materialized stdlib |
| `ide` | the features, as functions of `(&Analysis, FileId, offset)` returning spans and strings; no protocol types |
| `line_index` | UTF-8 offsets ⇄ UTF-16 positions |

**Projects.** A project is a directory with `k.toml`. On `initialize`, each workspace folder that is inside
a project opens that project; any other folder is searched (four levels, skipping hidden directories,
`target`, `dist` and `node_modules`) for projects. A document outside every known project joins the project
of its nearest ancestor with `k.toml`, loaded then, or else a *loose* project rooted at its directory that
holds only open documents. Each project has its own host, fed every analysis input (`k.toml`, `App.k`,
lockfiles, `.k` files) as `k check` loads them.

**Documents override the disk.** An open document's text is the file's text in the analysis; `didClose`
reverts to the disk (or removes a file that isn't there); `workspace/didChangeWatchedFiles` reloads files
that aren't open. When the client allows dynamic registration, the server registers watchers for `**/*.k`,
`**/k.toml` and `**/*.lock` after `initialized`. Synchronization is **incremental** (`change: 2`): each
`contentChanges` range is converted with a line index of the current text and spliced in, in order.

**Snapshots, staleness and cancellation.** Every change is applied by the main loop before the next message
is read, so a request sees every edit sent before it. A request runs on a worker with a snapshot taken when
it arrived. If an edit reaches its project while it runs, the snapshot answers `Cancelled` and the request
gets `ContentModified` (-32801), which clients retry. `$/cancelRequest` marks a request; when its result
arrives it is replaced by `RequestCancelled` (-32800). A request is never answered twice.

**Diagnostics are debounced.** A change marks its project dirty and restarts a timer (30 ms). When the input
has been quiet that long, a worker computes the project's diagnostics on a snapshot; they are published for
every file whose list changed, an empty list clears a file that had some, and every open document gets a
list (possibly empty) so the editor knows it was checked. A result computed for an older generation of the
project is dropped, since the newer one is on its way. On `initialized`, every project's diagnostics are
computed at once.

**The stdlib** is embedded in the compiler and has no files. For go-to-definition into it, its sources are
written once per process to `$TMPDIR/k-stdlib-<version>-<content hash>/`, and stdlib spans map to those
files. Requests on those files answer nothing (they are not project files), and they never create a loose
project.

**What k_sema provides** (sema.md [§17](sema.md#17-tooling-queries)): `file_references` (the reference under a
cursor), `references` (the project-wide reverse index, incremental per file), `definition`,
`implementations`, `page_url`, besides `resolve_at`, `type_at`, `type_info_at`, `program` (the declarations,
members and signatures) and `file_diagnostics`. Imports and doc links record resolutions, so references and
rename cover them.

### 2.1 Workspaces

A project that is a member of a workspace
([40-workspaces-and-services.md](../design/40-workspaces-and-services.md)) is loaded with the libraries
it depends on mounted (`k_sema::Workspace::analysis_files`; `crates/k_lsp/src/workspace.rs`,
`Membership`), and a folder holding a workspace root opens every member. Locations in a mounted file are
the library's own file on disk (`Project::file_path`), so definitions jump into `pay-contracts/`, not a
copy. A library document is one document for every project: `didOpen`/`didChange`/`didClose` and watched
file changes reach its own project and every member that mounts it (`Workspace::locate_all`), so an edit
of a contract re-checks its callers at once. Each project publishes only its own files' diagnostics; a
library's are published by the library's project. `textDocument/implementation` also asks every sibling
member that sees the same library file (`handlers::implementation_across`, matching the declaration by
qualified name and the member by name and labels), so go to implementation on `Payments.charge` in
shop finds paybox's handler. Tested by
`the_whole_workspace_is_indexed_and_implementations_cross_members` (`crates/k_lsp/tests/server.rs`).

## 3. Protocol coverage

| Method | Notes |
|---|---|
| `initialize`, `initialized`, `shutdown`, `exit` | `positionEncoding: utf-16`; exit code 0 after `shutdown`, else 1 |
| `textDocument/didOpen`, `didChange` (incremental), `didClose`, `didSave` | [§2](#2-architecture) |
| `workspace/didChangeWatchedFiles` | reloads files that aren't open; registered dynamically when allowed |
| `textDocument/publishDiagnostics` | debounced, project-wide ([§5](#5-diagnostics-and-quick-fixes)) |
| `textDocument/codeAction` | the quick fixes of the diagnostics in the range |
| `textDocument/definition`, `typeDefinition`, `implementation` | [§6](#6-navigation-references-and-rename) |
| `textDocument/references`, `documentHighlight` | project-wide reverse index |
| `textDocument/prepareRename`, `rename` | project-wide; rejects invalid names and stdlib symbols |
| `textDocument/hover` | [§7](#7-hover) |
| `textDocument/completion` | triggers `.`, `@`, `(` and `:`; `(` and `:` answer only in `.on(` and CSS values ([§8](#8-completion-and-signature-help)) |
| `textDocument/signatureHelp` | triggers `(` and `,`, retrigger `:` |
| `textDocument/documentSymbol` | hierarchical when the client supports it, flat otherwise |
| `workspace/symbol` | fuzzy, every project, at most 256 per project |
| `textDocument/inlayHint` | each page's derived URL after its name (`at "/sign-in"`) |
| `textDocument/formatting`, `rangeFormatting` | `k_fmt::format` as line edits (`ide::format`): lines aligned by their text without whitespace, so a range takes only its own lines' edits, and none when the partial result wouldn't format to the same text (a half-moved `init()`); `null` for a file with syntax errors; the client's options are ignored |
| `$/cancelRequest` | [§2](#2-architecture) |

Anything else: requests answer `MethodNotFound`, notifications are ignored. Formatting is tested by
`formatting_and_range_formatting_use_the_formatter` (`crates/k_lsp/tests/server.rs`) and `ide::format`'s
unit tests.

## 4. Positions

The protocol counts columns in UTF-16 code units; the analysis counts UTF-8 bytes. `LineIndex` converts
between them for one text. Lines end at `\n`, `\r\n` or a lone `\r`, as the lexer and the protocol agree.
Conversions never fail: a column past the end of a line clamps to the line's end, a line past the end of the
text to the end of the text, a column inside a surrogate pair snaps to the start of the character, and a
reversed range is put in order. Line indexes are built per snapshot on demand and cached for the request.

## 5. Diagnostics and quick fixes

A compiler diagnostic ([diagnostics.md](diagnostics.md)) maps to an LSP diagnostic as follows: the primary
span is the range; `Error`/`Warning` the severity; the code (`K2001`) the `code`; `source` is `k`; notes and
helps are appended to the message as `note: …` and `help: …` lines; **labels become `relatedInformation`**
with their own locations (in any file); codes whose registry name starts with `unused-` get the
`Unnecessary` tag, which editors render faded.

The server publishes the semantic pass's diagnostics, the `rpc.lock` and `storage.lock` checks included.
The `schema.lock`, migration and `routes.lock` checks (K6004–K6006, K8004–K8007) need code generation's
persistence model and route table, so they come from `k check` and `k build` only
([sema.md §12.16](sema.md#1216-lockfiles)); `k_lsp` doesn't depend on `k_codegen`.

`textDocument/codeAction` recomputes the file's diagnostics (memoized in the snapshot) and offers every
structured fix of every diagnostic whose span overlaps the requested range, as a `quickfix` with a
`WorkspaceEdit` (a fix may edit several files). A diagnostic's only machine-applicable fix is
`isPreferred`, so "fix all" style commands can apply it.

## 6. Navigation, references and rename

Everything starts from **the symbol under the cursor** (`ide::symbol_at`): the innermost resolved
reference whose range contains the cursor (a cursor just after a name counts, a reference the cursor is
inside wins over one it ends), else the declaration or member whose name is there, else the local or
parameter binding (found among the file's references when it is used, from the syntax otherwise). The
symbol is a `Resolution`, the key of the reverse index.

* **Definition**: `Analysis::definition`: a declaration's or member's name token (stdlib ones too), a binding,
  a theme token, a style class or keyframes rule. A page case (`.signIn`) goes to its page (`SignIn`).
  Built-in elements, attributes, events and CSS properties have no Kobra source.
* **Type definition**: the declaration of the type of the expression, binding, field, parameter or function
  result, through optionals and into the element type of a stdlib generic (`List<Todo>` goes to `Todo`).
* **Implementation**: for an interface or protocol, every declaration implementing it, transitively through
  other interfaces (a protocol's server service included); for a member of one, the members with the same
  name, kind and labels in each implementor and its extensions.
* **Implementation** also crosses the members of a workspace (§2.1).
* **References** and **highlights**: the reverse index; the declaration is included on request. A member
  of an interface or protocol and the members implementing it are **one symbol, as for rename**
  (`navigation::member_family`): find-references on an interface method lists the implementing methods
  (their names) and every call, through the interface or through a concrete type; on an implementing
  method, the interface method, the other implementations and their calls.

**Rename** replaces the declaration's name and every reference the index holds (code, imports, doc links)
with plain text edits, each checked to spell the old name (sema.md §17 guarantees they do). It also covers:

| Case | What else is renamed |
|---|---|
| an interface member, or a member implementing one | the whole family: the interface members it implements and every implementing member, with their references |
| a page | its cases in `Page` and `Endpoint` (`.signIn` for `SignIn`), in lower camel case |
| a public declaration named like its file | the file (`Square.k` → `Box.k`), as a `RenameFile` operation when the client supports resource operations (K1002 requires the names to match) |
| a function parameter without a separate label | the label is kept: `a: Int` becomes `a b: Int`, so callers stay valid |

`prepareRename` (and `rename`) refuse, with a message the editor shows: names that are not ASCII
identifiers, hard keywords and `_`; stdlib declarations; built-in elements, attributes, events and CSS
properties; generated members other than through their source; anything without a resolution.

## 7. Hover

The hover of a symbol is Markdown:

1. its declaration as Kobra source in a fenced block: annotations, visibility, keyword, qualified name,
   generics, signature or type, `implements`; a member names its owner (`// in shop.Cart`);
2. facts: **a page's URL** (`URL: /sign-in`, from `Analysis::page_url`), a declaration's side of the
   server/client boundary when it isn't shared, and for a member marked `@UserAction` (at its declaration
   or a call) that it **needs a user's action** (a click, submit or key handler, or a component event
   declared `@UserAction`, before the first `await`; K4030, 45-web-platform.md, "User activation";
   `hover::user_action_note`);
3. for a `@Derived` field or an `@Effect`, its **reactive dependencies** (design 03-reactivity.md: "the IDE
   shows an effect's dependencies on hover"): the references in its body to reactive members (`@State`,
   `@Derived`, `@Binding`, `@Prop`, `@Param`, `@Load`, computed properties), in source order, as written
   (`todos.remainingCount`), leaving out reads inside `untracked { … }` and plain assignments; an
   `@Effect(on: [a, b])` shows its explicit list; an effect that reads nothing says it never re-runs;
4. its doc comment, rendered: intra-doc links (``[`Name`]``, ``[`Type.member`]``) become Markdown links to
   their declarations (`file:///…#L12`), using the resolutions the checker recorded for each segment; a
   link that doesn't resolve stays code.

Built-in elements show the HTML they render, attributes their type and documentation from the element
table, CSS properties their value grammar. A **theme token** (`.muted` in a style block or a view) shows its
declaration with its value (`color muted: Color = #475569 // in ui.KanbanTheme`), the values the theme's
variants give it (``Variants: `dark`: `#94A3B8` ``) and its doc comment; a **class** (`.class(.count)` or a
selector) shows its rule from the style block, dedented and cut at 24 lines. Go to definition goes from a
token to its declaration in the theme (a project theme's before the stdlib's) and from a class to its rule's
selector. Off a symbol, hover shows the type of the innermost typed
expression.

## 8. Completion and signature help

The context is read from the text and the syntax before the cursor:

| Context | Items |
|---|---|
| after `@` | annotations, from the stdlib annotation table |
| after `receiver.` / `receiver?.` | the members of the receiver's type (recorded by the checker even when the member name is missing), its extensions' and interfaces'; static members and cases when the receiver names a type; private members only inside their declaration |
| after `.on(` (or `.on(` + prefix) | the receiver's events, labelled `edit:` (inserting `edit: `), with the payload type as detail and the doc comment as documentation: a component's `@Emits` fields (stdlib components such as `ReorderableList` too), a built-in element's events from the element table. The receiver is the base of the modifier chain the call ends (`CardView(…).class(.x).on(`), resolved by the checker or, in half-typed code, by name. Events another `.on(…)` of the chain handles are left out (K3104); after a first event's comma nothing is offered, since a call takes one event (K3108) |
| in a `style` block | read by a scanner over the block's text up to the cursor, which tracks rules, groups (`@media`, `@container`, `@layer`), `@keyframes`, strings, comments and interpolations: at a declaration's start (or on the line after a declaration missing its `;`) every CSS property, with its CSS name as detail and its grammar as documentation; after `property:` the property's camel-case keywords (`css::keywords`, their CSS spelling as detail), the CSS-wide keywords, and the theme tokens of the property's value types labelled `.md`, with type, value and theme as detail (`Spacing = 16px (ui.KanbanTheme)`), a project theme's before the stdlib's of the same name; at a selector's start the component's classes (`.column`); in `@media (` the media features, after `feature:` their keywords; in `@keyframes`, `from` and `to`. Inside an interpolation (`{accent}`) the names in scope |
| after `.` in a selector | the classes the component's style block declares and those its view references (marked "no rule yet") |
| after `.` in a class reference | the classes the enclosing declaration's style block declares: arguments of `.class(…)`, alone, in a list or before `when`, and of a child component's `@Stylable` prop |
| after `.` with no receiver | the implicit members of the expected type: the checker's type of the implicit member expression, else a CSS property's value types (`padding: .` lists `Spacing` tokens of the project's themes), an assignment target, the other operand of a comparison, a `let`'s or field's declared type, an argument's parameter, a `return`'s function result, a `case` pattern's scrutinee |
| elsewhere | locals and parameters bound before the cursor, the enclosing declaration's members, the module's declarations, imports, the prelude, built-in elements inside a `view`, keywords; public declarations of other modules with an `additionalTextEdit` adding the import |

Items are filtered by the identifier before the cursor (its characters in order, ignoring case; a leading
`.` of a token label is ignored, and `filterText` drops it and an event label's `:`) and sorted by group,
then name; the list is complete for the prefix (`isIncomplete: false`).

**Triggers.** `.` and `@` always complete. `(` and `:` are trigger characters too, but a completion they
trigger answers only in the contexts above that they start (an `.on(` call, a CSS property's value, a
media feature); anywhere else it answers nothing, so typing a call or a type annotation doesn't open a list
of every name in scope. Invoking completion explicitly there gives the names in scope as before.

**Signature help** finds the innermost call whose argument list contains the cursor (an unclosed list
counts), resolves its callee through the checker's recorded resolution, and shows a function's signature, a
type's initializers, a type's memberwise initializer (its settable stored fields) or a built-in element's
forms. The active parameter follows the argument's label, or its position. A callee written with
explicit type arguments (`decode<User>(`, `KeyName<AesGcmKey>(`) labels the signature with them, its
generic parameters replaced by the text written (`render::ty_with`); hover on such a name adds "Called as
`decode<User>(_ text: String) -> User`" below the declaration. A `@UserAction` member's signature carries
hover's user-action note as its `documentation`.

## 9. Robustness

* **Never panics on input.** A body that isn't JSON gets `ParseError` (id `null`); JSON that isn't a
  message gets `InvalidRequest` (with its id when it has one); a header block without a usable
  `Content-Length` is skipped; bodies over 64 MiB are skipped and refused. Parameters that don't deserialize
  get `InvalidParams`; requests before `initialize` `ServerNotInitialized`; after `shutdown`
  `InvalidRequest`. Positions outside a document clamp. Documents in no project answer `null`.
* **Half-typed code** is the normal case: every feature reads the parser's recovered tree and the checker's
  facts about it, and answers what it can (`ide::tests::half_typed_code_never_panics` queries every offset
  of broken inputs).
* **Contained bugs.** Workers run each request under `catch_unwind`; a panic (a bug) answers
  `InternalError` and the server keeps serving. The fuzz target and the property test fail on any such
  answer.

## 10. Testing and budgets

| Suite | What |
|---|---|
| `crates/k_lsp/src/**` unit tests | the features on fixture projects (`$0` marks the cursor); transport framing; UTF-16 conversion; incremental splicing; workspace discovery |
| `crates/k_sema/tests/tooling.rs` | the tooling queries; the property that every definition is spelled like its references, over the examples and well-typed generated projects |
| `crates/k_lsp/tests/server.rs` | the server in process: lifecycle and exit codes, malformed frames, sync with the disk, loose projects, definition into the stdlib, and go-to-definition at **every identifier of every example** answering a declaration spelled like it |
| `crates/k_lsp/tests/properties.rs` | random edit sequences and requests (some cancelled) through the server on example documents: one well-formed response per request, and the published diagnostics converge on a fresh `k check` of the edited text |
| `crates/k_cli/tests/lsp.rs` | end to end: spawns `k lsp` and drives it over JSON-RPC against the examples (definition, type definition, implementation in shop and notes, references, rename with file rename, hover with reactive dependencies and URLs, completion, signature help, symbols, diagnostics and quick fixes, inlay hints, malformed input) |
| `crates/k_lsp/tests/editors.rs` | the TextMate grammar compiles and highlights every lexer keyword; the Vim syntax files list them; the VS Code manifest is wired |
| `crates/k_lsp/tests/textmate.rs` | the TextMate grammar run by a small TextMate engine (begin/end/match/captures/include, earliest match first, `end` winning ties, line by line): scopes asserted on fixtures in `tests/textmate/` (test names, apostrophes, interpolations with nested braces and strings, doc comments, annotations, generics, view and style blocks), and agreement with `k_syntax::lex` on which tokens are strings, comments and code over every lexer-clean `.k` file of the examples, the stdlib and `tests/corpus` |
| `fuzz/fuzz_targets/lsp_edits.rs` | the property test's driver under libFuzzer, one server per process |
| `crates/k_lsp/tests/perf_budget.rs` | the latency budgets below (ignored; `scripts/bench-check.sh --lsp`) |

Budgets ([performance.md §12](performance.md#12-language-server), 100k-line realistic project, release build):

| Check | Budget |
|---|---|
| hover, warm | p50 ≤ 2 ms, p95 ≤ 10 ms |
| go to definition, warm | p50 ≤ 2 ms, p95 ≤ 10 ms |
| find references (project-wide), warm | p50 ≤ 5 ms, p95 ≤ 20 ms |
| completion, warm | p95 ≤ 50 ms (architecture.md §12) |
| completion after every `:`, `(` and `.` of style blocks and views, warm | p95 ≤ 50 ms (architecture.md §12) |
| hover as the first request after a body edit (re-check and re-index included) | ≤ 60 ms |
| diagnostics after a keystroke, end to end with the 30 ms debounce | p95 ≤ 100 ms (architecture.md §12) |

## 11. Editors

| Directory | Contents |
|---|---|
| `editors/vscode` | `package.json`, `extension.js` (vscode-languageclient, runs `k lsp`), `language-configuration.json`, `syntaxes/k.tmLanguage.json`, `README.md`. npm is used only here; nothing in the compiler's build or tests needs it, and no `node_modules` is committed |
| `editors/nvim` | `ftdetect/k.lua`, `ftplugin/k.lua`, `syntax/k.vim`, `lua/k/lsp.lua` (`require("k.lsp").setup()`: `vim.lsp.config` on 0.11+, nvim-lspconfig before) |
| `editors/vim` | `ftdetect/k.vim`, `ftplugin/k.vim`, `syntax/k.vim` (identical to Neovim's), `plugin/k_lsp.vim` (vim-lsp registration) |

The TextMate grammar's regexes avoid lookaround (and `\G`) so the Rust tests can evaluate them with the
`regex` crate; `editors.rs` fails when the lexer gains a keyword the grammar doesn't highlight, and
`textmate.rs` when the grammar's strings, interpolations or comments drift from the lexer's. A string ends at
its closing quote or at the end of its line, as in the lexer, so an unterminated string never colours the
rest of the file; an interpolation's `}` is found by balancing the braces inside it. The book chapter
[Editor setup](../book/src/getting-started/editor-setup.md) is the user-facing guide.

## 12. Limits and next steps

* **Static reactive dependencies** are an approximation of the runtime's tracking, as the design says:
  conditional reads are listed, reads inside helper functions are not.
* **Completion of locals** is syntactic: an `if let` binding is also offered in its `else` branch (the
  checker reports a misuse).
* **Generated members** other than page cases (`copy(…)`) have no definition and can't be renamed.
* **A test suite named like its declaration** shares its display name; resolutions name the declaration.
* Not yet: semantic tokens, code lenses (`k test` per test), call
  hierarchy, `workspace/willRenameFiles`, rename of a module directory, progress reports for the first load.
