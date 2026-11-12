# Documentation Tooling

How the book in `docs/book` is checked and generated, and where each planned check lives. The decisions
and the reasons are in [docs/design/49-documentation.md](../design/49-documentation.md); this file is for
people changing the tests and generators. Items marked **Planned** aren't built.

## 1. What exists

| Check | Where | Regenerate |
|---|---|---|
| Every ` ```k ` block compiles (`ignore`, `fails=`, `standalone`, hidden `~` lines, project files, excerpts) | `crates/k_sema/tests/book.rs` | — |
| The diagnostics page matches the code registry, the corpus examples and `check/diagnostic-fixes.txt` | `crates/k_corpus/tests/book_reference.rs` | `K_BLESS=1 cargo test -p k_corpus --test it` |
| One page per stdlib module from `///` comments, each listed in `SUMMARY.md` | same | same |
| `mdbook build docs/book` is warning-free; a missing chapter fails (`create-missing = false`) | run by hand | — |
| `{{k_version}}` is replaced with the toolchain's minor (`0.1`) before a block is checked | `crates/k_sema/tests/book.rs` | — |
| Links and anchors in `docs/book`, `docs/design`, `docs/compiler` resolve; a chapter's link stays inside the book; `[output.html.redirect]` targets are pages | `crates/k_sema/tests/book_links.rs` | — |
| `Planned (id)` / `Preview (id)` notes name rows of `k_sema::features` (`crates/k_sema/src/features.rs`) with the same status; every planned row is named by a note | `book.rs` | — |
| The Planned table of `reference/design-status.md` is generated from the notes, between `<!-- gen:planned … -->` and `<!-- /gen -->` | `book.rs` | `K_BLESS=1 cargo test -p k_sema --test it -- book::` |
| No `docs/book/check/stale.txt` pattern occurs in prose or in `text`, `bash`, `sh`, `console` and `toml` blocks (the diagnostics page, which shows mistakes on purpose, is skipped) | `book.rs` | — |
| Inline code that reads as a qualified K name (`RemoteClient.at`, `k.auth.Session`) or an annotation (`@Load`) resolves in the stdlib or the annotation registry; `docs/book/check/inline-names.txt` lists deliberate non-symbols, and an entry that resolves or is unused fails | `book.rs` | — |
| Every `k …` line in `bash`/`sh`/`console` blocks, and inline `` `k …` `` code outside Planned paragraphs, parses with `Cli::try_parse_from`; `# planned` lines must not | `crates/k_cli/tests/book_commands.rs` (includes `src/cli.rs` by path) | — |
| Probes: a planned command (`k new`, `k fmt`, `k doc`, `k explain`, `k add`, `k proto export`) doesn't parse; when it does, the test lists the notes to remove | `book_commands.rs` (`PROBES`) | — |
| `reference/web-platform.md` is generated from `docs/book/data/web-apis.toml`, between `<!-- gen:web-platform … -->` and `<!-- /gen -->`; every row's K API resolves in the stdlib, every file of `crates/k_runtime/js/runtime` is claimed by a row or listed under `[internal]`, and every Chromium or Firefox claim names a `#[test]` of `crates/k_cli/tests/chrome.rs` or `firefox.rs` (§6) | `crates/k_sema/tests/web_platform.rs` | `K_BLESS=1 cargo test -p k_sema --test it -- web_platform::` |
| `k --version`, `editors/vscode/package.json`, the examples' `k = "…"`, toolchain image tags (examples, `38-deployment.md`, the book) and the book's `k = "…"` and `k X.Y.Z` agree with `Cargo.toml` (or are `{{k_version}}`) | `crates/k_cli/tests/version.rs` | — |

## 2. Planned checks and where they go

All of them are `cargo test`s, so they need no CI vendor. Book-content checks that only need the analysis
stay in `k_sema`'s `book.rs`; checks that need the `k` binary, a project on disk or the clap definitions go
in `k_cli`'s tests; generated pages stay in `k_corpus`'s `book_reference.rs`, which already has the bless
flow.

| Check | File | Notes |
|---|---|---|
| Every page path of the last release's `SUMMARY.md` is a page or a redirect | same | the old list is committed as `docs/book/check/released-pages.txt`, rewritten at release |
| `k,test` blocks run; `k,render` blocks match their `html,output` block | `book.rs` | uses the test runner and the SSR renderer the `k.test` `render` function uses |
| `console` transcripts run against a copy of the named project | `crates/k_cli/tests/book_transcripts.rs` | normalise the temp path to `<project>`, durations to `<time>`, ports to `<port>`; `<!-- apply: … -->` patches applied first |
| Learn projects in `examples/learn/`: `k check`, `k build`, lockfiles (via `tests/examples.rs`); step diffs shown on the page; introduction order | `crates/k_cli/tests/book_projects.rs` | screenshots via the `dist_browser.rs` Chrome harness, skipped without Chrome unless `K_REQUIRE_CHROME=1` |
| Generated annotations, elements, style properties, keywords, operators, CLI, `k.toml` keys, diagnostics per family, `symbols.json` | `book_reference.rs` | `<!-- keep -->` regions survive regeneration; `SUMMARY.md` entries between `<!-- gen:… -->` markers |
| Coverage: doc string on every annotation, element, property, CLI flag | `book_reference.rs` | stdlib `///` coverage follows `39-doc-comments.md` |
| Probes for planned features that aren't commands (`iso-4217`: the currency table has more than five entries) | `book.rs` | the CLI probes are built |
| Every `SUMMARY.md` page has an entry in `docs/book/check/audit.toml` | `book_links.rs` | see §5 |
| Each built `docs/design` topic has a `> **Book:**` link to an existing page | `book_links.rs` | |
| Docs gate: each user-visible changelog fragment's `book:` pages are in the diff | `cargo xtask docs-gate <base>` (the `xtask` crate of `48-releases-and-versioning.md`) | not a test (needs a base ref); its rule matching is unit-tested on fixture diffs |

## 3. Sources of generated pages

The generators read the compiler's own data, never a copy:

- annotations: `crates/k_sema/src/stdlib/annotations.rs`
- built-in elements: `crates/k_sema/src/stdlib/elements.rs`
- style properties: `crates/k_sema/src/stdlib/css.rs`
- keywords and operator precedence: the tables in `k_syntax` the lexer and parser use
- diagnostics: `crates/k_diagnostics/src/codes.rs`, `tests/corpus`, `docs/book/check/diagnostic-fixes.txt`
- the CLI: `clap::CommandFactory::command()` on `k_cli::cli::Cli`
- the stdlib: `///` comments in `crates/k_sema/stdlib`

Where a registry has no doc string per entry, adding one is part of generating its page, and the
coverage check keeps it filled in.

## 4. Highlighting (Planned)

The `xtask` crate's mdBook preprocessor (which also substitutes `{{k_version}}`, `48-releases-and-versioning.md`) implements mdBook's preprocessor protocol (a `supports <renderer>` probe, then
the book as JSON on standard input, returned on standard output). It replaces each ` ```k ` block's body
with HTML spans from `k_syntax`'s lexer, keeping hidden `~` lines hidden. A test runs it over every book
block and checks that the token classes match the lexer's, so highlighting can't fall behind the grammar
the way `theme/k-highlight.js`'s copied keyword lists can.

## 5. The page audit

Round 1(a) of the plan in [49](../design/49-documentation.md#implementation-plan). Its aim is to find stale
or wrong **prose**, which no existing test reads; snippets are already compiled.

**Setup.** Pick one commit after the concurrent language work has landed and record it; audit against a
`k` built from it. Land the checks of round 1(a) items 1–4 first, so the audit starts from what they
report.

**Batches** (independent, so they can run in parallel; each one change):

| Batch | Pages |
|---|---|
| 1 | `introduction.md`, `philosophy.md`, `tour.md`, `getting-started/*` |
| 2 | `language/*` |
| 3 | `apps/` user interface: components, views, styles, reactivity, routing, async-data, forms, metadata, local-storage, charts, drag-and-drop, files, keyboard-shortcuts |
| 4 | `apps/` server: services, protocols, streaming, persistence, auth, config, endpoints, jobs, email, http |
| 5 | `apps/` rest: crypto, money, time, testing, packages, workspaces, app, deployment |
| 6 | `examples/*` against `examples/<name>/` |
| 7 | `reference/*` that isn't generated yet (keywords, annotations, operators, cli, project-files, design-status) |

**Per page, every claim of these kinds is checked against its source:**

| Claim | Source of truth |
|---|---|
| a command, flag, default or exit status; command output | `crates/k_cli/src/cli.rs`; run it on a scratch project |
| a status word: "not yet", "currently", "for now", "will", "planned", "isn't built" | the code; then a `Planned (id)` note or the word removed |
| behaviour: what renders, what runs when, what is rejected, what an error maps to | the design topic, then the test that pins it (`k_runtime/js/tests`, `k_server/tests`, `tests/corpus`) |
| a diagnostic code and what it means | `crates/k_diagnostics/src/codes.rs` and its corpus case |
| a stdlib or annotation name, a member, a label | `crates/k_sema/stdlib`, `stdlib/annotations.rs`, `elements.rs`, `css.rs` |
| a hand-copied table | the registry it copies (until round 2 generates it) |
| an example page's code and run instructions | the project in `examples/`, its README and `tests/examples.rs` |
| a lockfile or `k.toml` format | the reader and writer in `k_sema::workspace` and `k_cli` |

**Each finding** is fixed in the page in the batch's change and labelled in the change description:
*checkable* (add the `stale.txt` pattern or test that would have caught it, in the same change),
*pinnable* (true but untested: add a `k,test` block or a runtime test, or list it under the page's
`unpinned` in the ledger), or *judgement*.

**The ledger**, `docs/book/check/audit.toml`:

```toml
[pages."apps/testing.md"]
commit = "9d74570"
date = "2026-11-09"
unpinned = ["a failing test's output names the file and line"]
```

`book_links.rs` fails when a `SUMMARY.md` page has no entry. `cargo xtask release prepare` lists the
pages whose `commit` predates the previous minor's branch cut and whose linked design topic changed
since; the release manager re-audits them before the candidate.

**Found by the round 1(a) checks** (2026-11-09, fixed in the same change): the link test reported four
broken anchors (`docs/design/44-channels.md` to `40-workspaces-and-services.md#rpclock`;
`docs/compiler/codegen.md` to `testing.md#32-the-run-suite`; `docs/compiler/server.md` to
`codegen.md#154-background-jobs` and `#152-configuration`, sections renumbered since); the three
`25-config-and-secrets.md#secrett` links resolve (the heading `` `Secret<T>` `` has that id). The
stale-phrase list reported the `k check`-runs-tests claims on `getting-started/first-app.md` and
`apps/testing.md`; the generated table replaced the one on `reference/design-status.md`. The Planned-note
test reported all 38 notes without an id (one, `getting-started/first-app.md`'s, also claimed the tests
weren't part of `k check`); the command test reported `apps/packages.md`'s `k add` and
`k new --library` lines (now `# planned`) and its prose naming `k add` outside the Planned section,
`getting-started/commands.md`'s `k fmt` paragraph (no Planned marker), and `apps/deployment.md`'s deferred
`k build --target edge` written as if it were a command. The inline-name test reported 18 names, all deliberate (JavaScript names, editor settings,
placeholders, planned modules) and listed in `inline-names.txt`.

**Found by the audit** (2026-11-09, against `0dd7625` and its working tree, with a `k` built from it; every
`SUMMARY.md` page has its `audit.toml` entry). By class:

- *Checkable* (each now fails `book.rs`, through a `stale.txt` pattern or a test):
  - **Warnings the compiler doesn't report**, written as built: logging a `password` field or a `Secret`
    (`apps/auth.md`, `apps/crypto.md`), a session write from a `GET` handler (`apps/auth.md`), an error
    payload carrying a server-only type or a third party's body (`apps/async-data.md`, `apps/http.md`,
    `apps/protocols.md`), an effect that reads nothing or reads after `await` (`apps/reactivity.md`),
    unnamed duplicate landmarks and a `.label` without the visible text (`apps/views.md`), undocumented
    package API (`apps/packages.md`). Each is now a Planned row (`password-log-lint`, `safe-method-lint`,
    `error-payload-lint`, `effect-lints`, `a11y-lints`, `doc-coverage`).
  - **Quick fixes that don't exist:** `K3012` and `K3017` (`apps/components.md`, `apps/reactivity.md`,
    `language/errors.md`), `K7503` (`apps/config.md`, `apps/testing.md`), `K4022` (`apps/endpoints.md`),
    `K2001` (`apps/config.md`, `language/strings.md`) and a returned `Query` (`apps/persistence.md`). The new
    test `quick_fix_claims_name_a_code_with_a_pinned_fix` makes every sentence that gives a diagnostic a
    quick fix name its code, and requires a corpus case pinning that code's fix; 21 sentences gained their
    code, and `named-status-as-other.diag` now pins `K2401`'s fix.
  - **Behaviour the runtime doesn't have:** rpc routes accepting `application/proto` (`apps/auth.md`,
    `apps/protocols.md`; it is 415, Planned `protobuf`); each test in a rolled-back transaction
    (`apps/persistence.md`, `apps/testing.md`, `tour.md`, `examples/notes/README.md`; every test gets a fresh
    in-memory database); `Broadcast` reaching other replicas over `LISTEN`/`NOTIFY` (`apps/deployment.md`,
    `apps/streaming.md`; in-process, Planned `broadcast-replicas`); identical streams sharing one connection
    across tabs (`apps/streaming.md`, Planned `stream-tab-sharing`); `K_PUBLIC_ORIGIN` required in production
    (`getting-started/first-app.md`, `apps/config.md`, `apps/deployment.md`, `examples/shop/README.md`; unset,
    it is `http://localhost:{PORT}`); a `fileField` element (`apps/views.md`); `requires` on a layout
    (`reference/keywords.md`, `K5014`); `config` and `migration` as "recent additions" (`reference/keywords.md`).
- *Pinnable* (true or now Planned, and pinned or recorded): ordering two `Length`s is `K2013` whatever the
  units (`language/units.md` said only mixed units, outside layout; now a `k,fails=K2013` block); `K2001`
  catches `reveal()` inside an interpolation but not through a local (`language/strings.md`, a
  `k,fails=K2001` block); modifiers, `run`-only-by-the-worker, job metrics, `storage.lock` recording the
  namespace, `version` in `dist/app`'s logs and the formatter's member order are design, not built
  (Planned `modifiers`, `job-run-check`, `observe-metrics`, `storage-lock-writing`, `app-version`, `k-fmt`).
  The true claims no test pins are each page's `unpinned` in `audit.toml`.
- *Judgement:* `k dev` doesn't print errors on rebuild, it reports their count and keeps the last good
  build (`getting-started/commands.md`, `first-app.md`, `installation.md`); "static binary" (it links the
  system C library) became "self-contained"; `k check`'s description gained the tests (`tour.md`,
  `examples/shop.md`); `k config`'s output shown as it prints (no "currently set" column); the failing-test
  output on `tour.md`; `TimeZone` and `Weekday` named on `apps/time.md` instead of "calendars complete the
  module"; camelCase keyword values and the `TimeOfDay`/`HttpStatus` literals on `philosophy.md`;
  `DATABASE_PASSWORD` on `examples/notes/README.md`; `reference/cli.md`'s build paragraph (minified, source
  maps, preloads).

**Where the code and the design disagree** (the pages follow the code; each needs a decision, then the
design topic or the code changes):

1. **Test isolation.** [19](../design/19-persistence.md) and [21](../design/21-testing.md) run each test in
   a transaction the app can't see, rolled back afterwards, with the app's outermost transaction a
   savepoint; the runner gives every test a fresh in-memory database instead (`op_k_test_reset`).
2. **`K_PUBLIC_ORIGIN`.** [25](../design/25-config-and-secrets.md) and [38](../design/38-deployment.md) make
   it required in production; `ServerConfig` defaults it to `http://localhost:{PORT}` in every environment.
3. **Revealed secrets.** [25](../design/25-config-and-secrets.md) tracks values computed from `reveal()`
   through local bindings, with a quick fix to `secret.matches`; `K2001` reads the compared expression only
   and has no fix.
4. **`K_PUBLIC_SOURCE_MAPS`.** *Resolved:* it is `ServerConfig.publicSourceMaps: Bool = false`, read
   through the checked configuration like `K_WORKER_HEAP_MB` and listed by `dist/app config`
   ([25](../design/25-config-and-secrets.md), [13](../design/13-tooling.md)).

**Already known** (from writing design 49): the `k check`-runs-tests claim on
`getting-started/first-app.md`, `apps/testing.md` and `reference/design-status.md`; broken anchors in
`docs/design/44-channels.md` (`40-workspaces-and-services.md#rpclock`), `32-http-client.md` and
`33-crypto.md` (`25-config-and-secrets.md#secrett`, three links), `docs/compiler/codegen.md`
(`testing.md#32-the-run-suite`) and `docs/compiler/server.md` (`codegen.md#154-background-jobs`,
`codegen.md#152-configuration`).

## 6. The web platform page

`reference/web-platform.md` says which browser APIs K wraps, through which K API, where it runs, which
engines the real-browser suite checks it in, and what K doesn't wrap and why. Its tables are generated
from `docs/book/data/web-apis.toml`; the prose above and below the `<!-- gen:web-platform … -->` markers
is hand-written. The decision is in [49](../design/49-documentation.md#the-web-platform-page), and the
statuses come from [45's inventory](../design/45-web-platform.md#inventory) and the topics it links:
the data never claims more than the design decided and the stdlib has.

Update the data, then regenerate the page with
`K_BLESS=1 cargo test -p k_sema --test it -- web_platform::`, when:

- **a binding is added to `crates/k_runtime/js/runtime`.** The test fails until a row claims the file
  (`runtime = ["name.js"]` on each API it wraps) or `[internal]` lists it with why it wraps no browser
  API (the renderer, the server half, codecs);
- **a wrapper is built or grows.** Move the row from `planned` to `supported` or `partial`, drop its
  `tier` (or `feature`), and name its declarations in `k` (`k.device.Clipboard`, or `event:appeared` for
  an event every element has), its `side` and its runtime files;
- **a real-browser test starts covering an API.** Add the test's function name to `chromium` or
  `firefox`; a name that isn't a `#[test]` of `chrome.rs` or `firefox.rs` fails. Name only tests that
  exercise the binding, not ones that merely load a page using it. WebKit has no column until the
  suite drives it;
- **a design topic decides for or against an API.** Add or change the row, with `design` pointing at
  the heading that decides it (the test checks the heading exists) and a one-line `note`.

The checks: rows are well formed (status, tier, side, required fields, design heading, guide page,
planned feature id), K APIs resolve, runtime files are all accounted for, engine claims name tests, and
the page matches the data. They read files only and run in milliseconds.
