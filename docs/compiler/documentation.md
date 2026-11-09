# Documentation Tooling

How the book in `docs/book` is checked and generated, and where each planned check lives. The decisions
and the reasons are in [docs/design/49-documentation.md](../design/49-documentation.md); this file is for
people changing the tests and generators. Items marked **Planned** aren't built.

## 1. What exists

| Check | Where | Regenerate |
|---|---|---|
| Every ` ```k ` block compiles (`ignore`, `fails=`, `standalone`, hidden `~` lines, project files, excerpts) | `crates/k_sema/tests/book.rs` | — |
| The diagnostics page matches the code registry, the corpus examples and `check/diagnostic-fixes.txt` | `crates/k_corpus/tests/book_reference.rs` | `K_BLESS=1 cargo test -p k_corpus --test book_reference` |
| One page per stdlib module from `///` comments, each listed in `SUMMARY.md` | same | same |
| `mdbook build docs/book` is warning-free; a missing chapter fails (`create-missing = false`) | run by hand | — |

## 2. Planned checks and where they go

All of them are `cargo test`s, so they need no CI vendor. Book-content checks that only need the analysis
stay in `k_sema`'s `book.rs`; checks that need the `k` binary, a project on disk or the clap definitions go
in `k_cli`'s tests; generated pages stay in `k_corpus`'s `book_reference.rs`, which already has the bless
flow.

| Check | File | Notes |
|---|---|---|
| Links and anchors in `docs/book/src`, `docs/design`, `docs/compiler`; redirect targets | `crates/k_sema/tests/book_links.rs` | heading ids computed as mdBook does: lowercase, drop punctuation except `-` and `_`, spaces to `-`, `-1`, `-2` for repeats |
| Every page path of the last release's `SUMMARY.md` is a page or a redirect | same | the old list is committed as `docs/book/check/released-pages.txt`, rewritten at release |
| `k,test` blocks run; `k,render` blocks match their `html,output` block | `book.rs` | uses the test runner and the SSR renderer the `k.test` `render` function uses |
| `console` transcripts run against a copy of the named project | `crates/k_cli/tests/book_transcripts.rs` | normalise the temp path to `<project>`, durations to `<time>`, ports to `<port>`; `<!-- apply: … -->` patches applied first |
| Every `k …` line in `bash`/`console` blocks parses with `Cli::try_parse_from`; `# planned` lines must not | `crates/k_cli/tests/book_commands.rs` | needs `k_cli`'s `Cli` reachable from tests (it is in `src/cli.rs`) |
| Learn projects in `examples/learn/`: `k check`, `k build`, lockfiles (via `tests/examples.rs`); step diffs shown on the page; introduction order | `crates/k_cli/tests/book_projects.rs` | screenshots via the `dist_browser.rs` Chrome harness, skipped without Chrome unless `K_REQUIRE_CHROME=1` |
| Generated annotations, elements, style properties, keywords, operators, CLI, `k.toml` keys, diagnostics per family, `symbols.json` | `book_reference.rs` | `<!-- keep -->` regions survive regeneration; `SUMMARY.md` entries between `<!-- gen:… -->` markers |
| Coverage: doc string on every annotation, element, property, CLI flag | `book_reference.rs` | stdlib `///` coverage follows `39-doc-comments.md` |
| `Planned` notes carry an id; probes for checkable ids; the design-status table is generated from the notes | `book.rs` (ids) and `book_commands.rs` (CLI probes) | a succeeding probe fails with the list of notes to remove |
| `stale.txt` patterns don't occur outside `k` blocks | `book.rs` | one pattern per line, `#` comment = reason |
| Inline-code text in prose that parses as a qualified K name or an annotation resolves in the stdlib or annotation registry | `book.rs` | allowlist in `docs/book/check/inline-names.txt` for deliberate non-symbols (other languages' names on "Coming from" pages) |
| Every `SUMMARY.md` page has an entry in `docs/book/check/audit.toml` | `book_links.rs` | see §5 |
| `Planned (id)` / `Preview (id)` ids exist in `k_sema::features` with the matching status | `book.rs` | the table starts with `planned` rows only (round 1) |
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

**Already known** (from writing design 49): the `k check`-runs-tests claim on
`getting-started/first-app.md`, `apps/testing.md` and `reference/design-status.md`; broken anchors in
`docs/design/44-channels.md` (`40-workspaces-and-services.md#rpclock`), `32-http-client.md` and
`33-crypto.md` (`25-config-and-secrets.md#secrett`, three links), `docs/compiler/codegen.md`
(`testing.md#32-the-run-suite`) and `docs/compiler/server.md` (`codegen.md#154-background-jobs`,
`codegen.md#152-configuration`).
