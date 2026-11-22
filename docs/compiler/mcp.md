# The MCP server

`k mcp` is a Model Context Protocol server for AI assistants: Claude Code, or any MCP client, runs it and
asks it about Kobra. It answers from the documentation and examples embedded in the `k` binary and from
the project it runs in, and it checks Kobra code with the compiler's own analysis. The decision is recorded
in [13-tooling.md, "AI assistants"](../design/13-tooling.md#ai-assistants); the user-facing page is the
book's `getting-started/ai-assistants.md`. The crate is `crates/k_mcp`.

Contents:

1. [Install](#1-install)
2. [Dependencies](#2-dependencies)
3. [Architecture](#3-architecture)
4. [The corpus](#4-the-corpus)
5. [Protocol](#5-protocol)
6. [Tools](#6-tools)
7. [Resources](#7-resources)
8. [Toolchains](#8-toolchains)
9. [Testing](#9-testing)
10. [Limits and next steps](#10-limits-and-next-steps)

---

## 1. Install

```bash
claude mcp add kobra -- k mcp
```

`--scope user` (after `add`) makes it available in every project. Any other client runs the command `k`
with the argument `mcp` over stdio; `k mcp --root <dir>` names the project instead of the one around the
working directory.

## 2. Dependencies

| Crate | Use |
|---|---|
| `serde_json` | the JSON-RPC messages and the tools' schemas and results |
| `flate2` | compressing the corpus in `build.rs` and inflating it on first use |
| `k_sema`, `k_diagnostics` | `check_snippet` (the analysis) and `explain_diagnostic` (the registry) |

No MCP SDK: the server implements a handful of methods over newline-delimited JSON-RPC, like the
language server's hand-written transport ([lsp.md §1](lsp.md#1-dependencies)). The Rust SDKs bring an
async runtime and a large dependency tree for that.

## 3. Architecture

```text
k mcp ──▶ k_mcp::run_stdio ──▶ server::serve (one line in, at most one line out)
                                   ├── initialize, ping
                                   ├── tools/list, tools/call ──▶ tools ──▶ corpus, search, explain, check
                                   └── resources/list, resources/read ──▶ corpus
```

| Module | Role |
|---|---|
| `server` | the message loop, method dispatch, JSON-RPC errors |
| `tools` | the tool definitions (names, descriptions, input schemas) and their calls |
| `corpus` | the embedded documents, the project's documents, the examples, line paging |
| `search` | term-based ranked search |
| `explain` | diagnostic codes, from the registry and the references |
| `check` | a snippet through `k_sema`'s `AnalysisHost` |

Requests are answered one at a time, in order; each takes milliseconds except `check_snippet`, which
takes what a check of a small program takes (the standard library is analysed with it).

## 4. The corpus

`crates/k_mcp/build.rs` walks the repository at build time and keeps:

| Corpus path | Source | Kept |
|---|---|---|
| `book/…` | `docs/book/src/…` | `.md` |
| `design/…` | `docs/design/…` | `.md` |
| `compiler/…` | `docs/compiler/…` | `.md` |
| `README.md` | the repository README (section `book`) | |
| `examples/…` | `examples/…` | `.md`, `k.toml`, `.k`; not lockfiles, `dist/`, `target/`, `node_modules/` or hidden files |

The files are concatenated into one blob, compressed with DEFLATE into `OUT_DIR/corpus.deflate` (about
1.5 MB for about 5 MB of text, embedded with `include_bytes!`), and indexed by a generated
`corpus_index.rs` (each path's byte range). The first request that needs a document inflates the blob
once (tens of milliseconds). The script prints `rerun-if-changed` for the docs directories and for each
example file and `src/` directory, not for whole example directories, whose `dist/` changes on every build.

**The project** is read from disk on every request, so edits show at once: `--root`, else the nearest
directory with a `k.toml` at or above the working directory. Its `.md`, `k.toml` and `.k` files are the
section `project` (`project/src/home/Home.k`), skipping hidden directories, `dist/`, `target/` and
`node_modules/`, at most 5,000 files of at most 2 MB each.

**Examples** are the directories below `examples/` with a `k.toml` that are not inside another one, so a
workspace (`shop`) is one example with its members, and `learn/quick-start` and each `learn/shelf/NN-*`
step are examples of their own. An example's title is its `k.toml`'s `title`, its summary the README's
first paragraph (else the manifest's `description`).

## 5. Protocol

The MCP stdio transport: one JSON-RPC 2.0 message per line on standard input, one per line on standard
output, nothing else on standard output (the one log line, at start, goes to standard error).

| Method | Answer |
|---|---|
| `initialize` | the client's `protocolVersion` if it is `2025-06-18`, `2025-03-26` or `2024-11-05`, else `2025-06-18`; capabilities `tools` and `resources` (no `listChanged`, no `subscribe`); `serverInfo` `kobra` with the toolchain's version; `instructions` telling the assistant to search the docs and check what it writes |
| `ping` | `{}` |
| `tools/list` | the seven tools, unpaginated |
| `tools/call` | `content` (one text block), `isError`, and `structuredContent` for `check_snippet` |
| `resources/list` | every document, unpaginated |
| `resources/templates/list` | none |
| `resources/read` | the document's text |
| notifications | ignored (`notifications/initialized`, `notifications/cancelled`, …) |

Errors: a line that isn't JSON is `-32700` (id `null`); JSON that isn't a request object, a batch
included, `-32600`; an unknown method `-32601`; an unknown tool or arguments that don't fit its schema
`-32602`; an unknown resource `-32002`. A tool that ran but couldn't do what was asked (no such
document, example or code) answers `isError: true` with a message that says what exists, so the model
can correct itself. Lines over 16 MB are skipped with `-32700`. Requests are answered before
`initialize` too.

## 6. Tools

| Tool | Arguments | Result |
|---|---|---|
| `list_docs` | `section?`: `book`, `design`, `compiler`, `examples` or `project` | each document's path and title (its first `# ` heading, else its file name), by section; without `section` the examples and project files are counted, not listed |
| `read_doc` | `path` (or a `kobra://` URI), `offset?` (1-based line, default 1), `limit?` (lines, default 400, at most 2,000) | the lines, under a header naming the range, and the offset to read on from |
| `search_docs` | `query`, `section?`, `limit?` (default 10, at most 50) | ranked matches: path, line, section heading and the line itself |
| `list_examples` | none | each example's name, title, summary and files |
| `read_example` | `name`, `file?`, `offset?`, `limit?` | the file, or the README, `k.toml` and every `.k` source under `=== <path> ===` headers, paged like `read_doc` |
| `explain_diagnostic` | `code`: `K2011`, `2011` or `assignment-to-let` | the registry's name, severity and summary; the code's section of `book/reference/diagnostics.md` (an example with the compiler's output, and the fix); the rows of `compiler/diagnostics.md` naming it (primary span, quick fix) |
| `check_snippet` | `source`, `file_name?`, `files?` (path to source) | `k check`'s human rendering of every diagnostic, and as `structuredContent` `{file, errors, warnings, diagnostics}` with `k check --format json`'s objects |

**Search** splits the query into lower-cased words and cuts each document into sections: Markdown at its
headings, other files every 40 lines. A section matches when it holds every word; when none does, the
sections holding the most words match. The score sums, per word, `1 + ln(occurrences)`, tripled when the
word is in the section's heading and doubled when it is in the document's path or title; book pages weigh
1.2 and project files 1.3 against the design and compiler docs, which say the same things for implementers.

**`check_snippet`** builds the program the book's sample check builds (`crates/k_sema/tests/book.rs`):
the snippet at `file_name`, else `App.k` when it declares an `app`, else `src/snippet/<Name>.k`
(`.test.k` with a `test`) after its first public declaration; the `files`; and, when nothing declares a
`theme`, `docs/book/check/BookTheme.k`. It runs `AnalysisHost::diagnostics` on it, as `k check` does for
a project. Tests and the codegen lockfile checks (`schema.lock`, `routes.lock`) don't run: they need a
whole project, and `k check` is the tool for one.

## 7. Resources

Every document is a resource: `kobra://` and its corpus path (`kobra://book/language/types.md`,
`kobra://design/13-tooling.md`, `kobra://examples/todos/App.k`, `kobra://project/App.k`), named by its
path, titled by its title, with `text/markdown`, `application/toml` or `text/plain` (Kobra sources) and
its size.

## 8. Toolchains

`k mcp` is never passed to a project's pinned toolchain, unlike `k lsp`
([48-releases-and-versioning.md](../design/48-releases-and-versioning.md)): an assistant's configuration
runs one `k mcp` for every project, and toolchains older than the command don't have it. The docs it
serves are therefore those of the `k` on the `PATH`.

## 9. Testing

`cargo test -p k_mcp` drives `k_mcp::serve` over in-memory streams (`crates/k_mcp/tests/server.rs`):
the handshake and version negotiation, `tools/list`, every tool on the real embedded corpus, a project
on disk, resources, and the protocol errors. `k_cli`'s `cli::mcp_answers_over_stdio` runs the real
binary once.

## 10. Limits and next steps

- `check_snippet` doesn't check a snippet inside the project the server runs in (with its other files and
  libraries); the assistant runs `k check` there. Doing it in the server would need the driver's project
  and workspace loading (`k_cli`'s `sources`) in a library crate.
- No `listChanged` notifications: the project's files are re-read on every request instead.
- Search is lexical; it has no stemming or synonyms.
