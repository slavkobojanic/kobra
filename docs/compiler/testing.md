# Testing

K is built test-first: tests for a stage are written before its implementation, and they must cover edge
cases exhaustively. This document explains every kind of test, how to run it, and exactly how to add
cases. Most of it can be followed without any implementation in place.

Contents:

1. [Test kinds at a glance](#1-test-kinds-at-a-glance)
2. [Running tests](#2-running-tests)
3. [Golden corpus](#3-golden-corpus)
4. [Unit and integration tests](#4-unit-and-integration-tests)
5. [Property tests](#5-property-tests)
6. [Fuzzing](#6-fuzzing)
7. [Regression tests](#7-regression-tests)
8. [End-to-end tests](#8-end-to-end-tests)
9. [Benchmarks and performance budgets](#9-benchmarks-and-performance-budgets)
10. [Planned suites](#10-planned-suites)

---

## 1. Test kinds at a glance

| Kind | Where | What it checks |
|---|---|---|
| Unit | `#[cfg(test)] mod tests` inside each source file | One function or type in isolation |
| Integration | `crates/<crate>/tests/*.rs` | A crate's public API |
| Golden corpus | `tests/corpus/<suite>/**/*.k` + expectation files | Exact output of a compiler stage for a K source |
| Property | `crates/<crate>/tests/properties.rs`, `invariants.rs` | Invariants over generated inputs (proptest) |
| Fuzz | `fuzz/fuzz_targets/*.rs` | No panics, no invariant violations on arbitrary and generated inputs (libFuzzer) |
| Regression | `tests/corpus/<suite>/regressions/` | A fixed bug stays fixed |
| End-to-end | `crates/k_cli/tests/`, later `tests/e2e/` and `examples/` | The `k` binary, built apps, HTTP, browser |
| Benchmark | `crates/<crate>/benches/*.rs` (criterion) | Throughput and latency against the budgets |

---

## 2. Running tests

Every command assumes `. "$HOME/.cargo/env"` has been run in the shell.

```sh
cargo test --workspace                          # everything that runs on stable
cargo test -p k_syntax                          # one crate
cargo test -p k_corpus --test corpus            # the whole golden corpus
cargo test -p k_corpus --test corpus -- lexer/  # corpus cases whose name contains "lexer/"
cargo test -p k_corpus --test corpus -- --exact lexer/strings/escapes.k
K_BLESS=1 cargo test -p k_corpus --test corpus  # rewrite expectations from actual output
cargo test -p k_syntax --test properties        # property tests (shared oracle, see §5)
cargo test -p k_syntax --test invariants        # the original minimal invariants
PROPTEST_CASES=20000 cargo test -p k_syntax --test properties   # longer property run
K_STRESS_CASES=100000 cargo test -p k_syntax --test robustness  # longer random stress run
cargo +nightly fuzz run parse                   # fuzzing (see §6)
cargo bench -p k_syntax                         # benchmarks (see §9)
cargo clippy --workspace --all-targets          # must be clean
cargo fmt --all --check                         # must be clean
```

Inspect what a stage produces for a file, in exactly the golden formats:

```sh
cargo run -q -p k_cli -- debug tokens path/to/file.k   # .tokens format
cargo run -q -p k_cli -- debug ast    path/to/file.k   # .ast format (canonical layout)
cargo run -q -p k_cli -- debug diag   path/to/file.k   # .diag format
```

---

## 3. Golden corpus

### 3.1 Layout

```text
tests/corpus/
  README.md
  lexer/                      ← suite directory
    README.md                 ← optional notes (.md files are ignored)
    strings/
      escapes.k               ← one case
      escapes.tokens          ← required expectation for the lexer suite
      escapes.diag            ← optional expected diagnostics
    numbers/units.k
    numbers/units.tokens
  parser/
    expressions/precedence.k
    expressions/precedence.ast
    errors/missing-paren.k
    errors/missing-paren.ast
    errors/missing-paren.diag
```

- Every **direct subdirectory** of `tests/corpus` is a suite. An unknown suite name fails the run.
- Every `*.k` file anywhere below a suite directory is **one case**. Subdirectories are only for
  grouping; nest them as deep as useful.
- Expectations are **siblings with the same stem**: `escapes.k` → `escapes.tokens`, `escapes.diag`.
- The test name is the path relative to `tests/corpus` (`lexer/strings/escapes.k`), so cargo's filter
  selects groups by directory.
- Layout mistakes are failing tests named `layout/<path>`: an expectation file without its `.k` file, an
  expectation extension the suite does not use (`.ast` in `lexer/`), or any other file type except `.md`.

| Suite | Runs | Required | Optional |
|---|---|---|---|
| `lexer` | `k_syntax::lex` | `name.tokens` | `name.diag` (lexer diagnostics) |
| `parser` | `k_syntax::parse` | `name.ast` | `name.diag` (lexer + parser diagnostics) |

**No `.diag` file means "no diagnostics".** Every diagnostic the stage produces must be listed.

Every case also checks losslessness: the token texts (lexer) or the tree text (parser) must reproduce the
source exactly.

### 3.2 Writing a case

1. Pick the suite and a directory for the topic (`lexer/strings/`, `parser/statements/`, ...).
2. Name the file after the behavior it pins down, in kebab-case: `unterminated-at-eof.k`,
   `trailing-lambda-in-if-condition.k`. One behavior per file; small files.
3. Write the source. **Mind the final newline**: most editors add one, and it appears in `.tokens` as
   `Newline "\n"`. A case about end-of-file behavior must *not* end with a newline (check with
   `tail -c1 file.k | xxd`).
4. Write the expectations by hand from `docs/compiler/grammar.md` (and `diagnostics.md` for spans). Do not
   bless output you have not reviewed.
5. Run `cargo test -p k_corpus --test corpus -- <your path>`. Before the stage is implemented, new cases
   fail; that is expected in test-first work.

Cover edge cases systematically for each feature: the empty form, the smallest valid form, every optional
part present and absent, maximal nesting, trivia (comments, blank lines, CRLF) in every position, the
end of file in the middle of the construct, and each diagnostic with its fix.

### 3.3 `.tokens` format (lexer suite)

One token per line: the `SyntaxKind` name, one or more spaces, and the token text as a quoted string.

- `Whitespace` tokens are **omitted** (listing one is an error). All other tokens are listed, including
  `Newline`, `LineComment`, `BlockComment`, `DocComment` and `Error`.
- Quoting uses `\\`, `\"`, `\n`, `\r`, `\t`, and `\u{HEX}` for other control characters; all other
  characters, including non-ASCII ones, are written literally.
- Blank lines and lines whose first non-blank character is `#` are ignored; use `#` lines for notes.
- Contextual keywords (`component`, `page`, `view`, ...) are `Ident` at the token level.

Example, `tests/corpus/lexer/strings/interpolation.k` containing `let a = "x{y}" // hi` and a final
newline:

```text
# interpolation splits the string into pieces
LetKw "let"
Ident "a"
Eq "="
StringStart "\""
StringText "x"
InterpStart "{"
Ident "y"
InterpEnd "}"
StringEnd "\""
LineComment "// hi"
Newline "\n"
```

The token kind names are listed in `crates/k_syntax/src/kind.rs` and described in
[grammar.md §3](grammar.md#3-lexical-structure).

### 3.4 `.ast` format (parser suite)

One S-expression: the dump of the whole tree, starting with `(SourceFile`. The dump rules are in
[grammar.md §19](grammar.md#19-syntax-tree-dump-format). In short:

- `(NodeKind children...)` for every node, in source order;
- tokens are bare atoms of their text (`count`, `var`, `+=`, `16px`, `#3B82F6`), except trivia and the
  structural punctuation `( ) [ ] { } , ; : . ? @ -> =>`, string quotes and interpolation braces, which
  are omitted;
- string contents are one quoted atom with the decoded value: `(StringLit "Hello, " (Interpolation
  (NameExpr name)) "!")`.

Layout is free and `;` starts a comment, because the comparison is structural:

```lisp
; tests/corpus/parser/declarations/component-minimal.ast
(SourceFile
  (ComponentDecl component Counter
    (MemberList
      (FieldDecl (Annotation State) var count (PathType Int) = (Literal 0)))))
```

On failure the harness prints a diff of both trees in canonical layout.

### 3.5 `.diag` format (all suites)

```text
# Lines starting with # are comments; blank lines are ignored.
error[K0002] 1:9-1:10: unterminated string literal
error[K0104] 2:6-2:8
  fix "Replace `&&` with `and`"
    edit 2:6-2:8 "and"
warning[K3001] 7:5-7:12: effect reads no reactive values
  label 6:5-6:12: declared here
  note: an effect re-runs only when a value it read changes
  help: read a `@State` or `@Derived` value, or remove `@Effect`
```

**Header line** (not indented): `severity[CODE] SPAN` optionally followed by `: message`.

- `severity` is `error` or `warning`; `CODE` is `K` and four digits.
- `SPAN` is `startLine:startCol-endLine:endCol`. Lines and columns are **1-based**, columns count
  **Unicode scalar values** (not bytes, not UTF-16), and the end is **exclusive**. An empty span (an
  insertion point) has equal start and end: `3:14-3:14`.
- Omitting `: message` makes the message a wildcard. Prefer omitting it unless the wording is what the
  case tests; codes and spans are the contract, messages may be polished later.

**Detail lines** (indented by any whitespace) belong to the header above them:

| Line | Meaning |
|---|---|
| `label SPAN` or `label SPAN: message` | A secondary label |
| `note: text` | A note |
| `help: text` | A help line |
| `fix "message"` or `fix "message" maybe-incorrect` | A quick fix (machine-applicable unless marked) |
| `edit SPAN "replacement"` | One edit of the preceding fix (quoted like `.tokens` text) |

If a header has **no** detail lines, the actual diagnostic's labels, notes, helps and fixes are not
compared. If it has **any**, all of them are compared exactly, in this order: labels, notes, helps,
fixes.

**Matching:** the order of entries in the file does not matter; each expected entry must match a distinct
actual diagnostic, and every actual diagnostic must be matched.

Span example: in `let s = "abc` (line 1), `l` is column 1 and the `"` is column 9, so an unterminated
string is `error[K0002] 1:9-1:10`. In `é = 1`, `=` is column 3 although `é` is two bytes.

Where each code's primary span goes is specified in [diagnostics.md](diagnostics.md).

### 3.6 Blessing

`K_BLESS=1` makes failing cases rewrite their expectations from the actual output (canonical layout;
a `.diag` file is created when diagnostics appear and deleted when none remain) and pass. Use it only
after a deliberate change whose new output you have reviewed, then review the diff in git. Corpus
expectations are normally written by hand.

### 3.7 Example cases

A lexer error case, `tests/corpus/lexer/strings/unterminated.k` (with a final newline):

```k
let s = "abc
```

`unterminated.tokens`:

```text
LetKw "let"
Ident "s"
Eq "="
StringStart "\""
StringText "abc"
Newline "\n"
```

`unterminated.diag`:

```text
error[K0002] 1:9-1:10
```

A parser recovery case, `tests/corpus/parser/errors/symbolic-and.k`:

```k
let ok = a && b
```

`symbolic-and.ast` (the parser treats `&&` as `and` for recovery):

```lisp
(SourceFile (FieldDecl let ok = (BinaryExpr (NameExpr a) && (NameExpr b))))
```

`symbolic-and.diag`:

```text
error[K0104] 1:12-1:14
  fix "Replace `&&` with `and`"
    edit 1:12-1:14 "and"
```

---

## 4. Unit and integration tests

- **Unit tests** live in a `#[cfg(test)] mod tests` at the bottom of the file they test. Name tests after
  the behavior: `offsets_past_end_are_clamped`, not `test_line_index_3`.
- **Integration tests** live in `crates/<crate>/tests/*.rs` and use only the public API. Start each file
  with a `//!` comment saying what it covers.
- Tests may `unwrap`, `expect` and `panic` (clippy allows it in tests). Production code may not.
- Build syntax trees by hand in unit tests with `rowan::GreenNodeBuilder` (see `crates/k_syntax/src/ast.rs`
  tests) when a test should not depend on the parser.
- **Grammar coverage against real code:** `crates/k_syntax/tests/examples.rs` parses every `.k` file of
  the example apps in `examples/` and fails on any syntax diagnostic. The examples are written against
  `docs/design`, not the parser, so a construct the design adopted but the grammar lacks fails here.
  When it does, add the construct to `grammar.md` and the corpus first, then to the parser.

---

## 5. Property tests

Property tests use [proptest](https://docs.rs/proptest). They state invariants that must hold for *every*
input, and run on stable as part of `cargo test`.

### 5.1 The syntax oracle

All syntax invariants are checked by one module, `crates/k_syntax/tests/support/oracle.rs`, which the
property tests *and* the fuzz targets (§6) include, so a property is never checked one way on stable and
another way under fuzzing. `oracle::check_all(text)` lexes and parses `text` and asserts:

| Check | Invariant |
|---|---|
| `check_lexed` | tokens are non-empty token kinds, contiguous from offset 0, on char boundaries, and concatenate to the input; fixed-text kinds (keywords, punctuation) have their spelling |
| `check_diagnostics` | primary spans, labels and fix edits lie in the file and on char boundaries; codes are registered; messages are one line; edits of one fix do not overlap; diagnostics are sorted |
| `check_tree` | the root is a `SourceFile` covering the input; the tree text equals the input exactly; the tree's tokens are the lexer's tokens (same ranges and texts, same kinds except contextual keywords the parser remapped from `Ident`, grammar §6) |
| `check_lexer_diagnostics_are_reported` | every lexer diagnostic is in the parse's diagnostics |
| `check_relex` | re-lexing the concatenated texts of the tree's tokens reproduces the token stream, offsets and diagnostics |
| `check_dumps` | the `.tokens` dump reads back to the same token lines; the `.ast` dump reads back to the same S-expression |
| `check_error_bound` | at most `2 × tokens + 2` diagnostics for `tokens` non-trivia tokens: one mistake must not cascade |
| `check_deterministic` | lexing and parsing again give identical kinds, offsets, green trees and diagnostics |

`oracle::check_fast` is the cheap subset (no dumps, no second parse) used by the throughput-oriented
`parse` fuzz target. `oracle::check_no_duplicates` (no diagnostic with the same code and span reported
twice) is part of `check_all`; the inputs that once violated it are `FORMER_DUPLICATES` in
`properties.rs` and the `duplicate-diagnostic-*` cases in `tests/corpus/parser/regressions/`.

### 5.2 Inputs

`crates/k_syntax/tests/properties.rs` runs the oracle over:

- **arbitrary text**: any Unicode string, and any byte string decoded lossily (invalid UTF-8 becomes
  U+FFFD, a non-ASCII error character);
- **K-shaped text**: fragments of real and broken K syntax concatenated (`FRAGMENTS`), which reaches the
  string, interpolation and style lexer modes. When a construct is added, add its fragments;
- **generated programs**: the structure-aware generator `crates/k_syntax/tests/support/kgen.rs` (§5.3)
  fed with proptest bytes; programs generated in *valid* mode must parse with **no diagnostics**;
- **derived inputs**: every prefix of a generated program (a cut at any char boundary models typing), and
  broken K-shaped text spliced into the middle of a generated program;
- the same text under a different `FileId`, which must change nothing but the spans' file;
- a fixed list of edge cases (empty input, lone BOM, CR, unterminated strings, comments and blocks, ...).

`PROPTEST_CASES=N` raises the number of cases per property (default 384). Failing inputs are shrunk,
printed, and persisted in `crates/k_syntax/tests/properties.proptest-regressions` (check that file in);
each finding should also become a corpus regression case (§7).

### 5.3 The structure-aware generator

`kgen::generate(&mut arbitrary::Unstructured)` walks the grammar of [grammar.md](grammar.md): imports,
every declaration kind with its clauses, members, statements, expressions (precedence, postfix chains,
trailing lambdas, `try`/`await`, `bind`, `when`), patterns, types, views with modifiers, style blocks
(selectors, declarations, interpolations, `@media`) and tests, with comments, blank lines and CRLF in
between. A node budget and a depth limit keep programs small (median about 2.5 KiB) and below the
parser's nesting limit.

The first choice picks the mode: *valid* (two thirds) writes only well-formed syntax, so any diagnostic is
a parser bug or a generator bug; *noisy* drops, doubles or prefixes tokens with broken fragments to reach
error recovery from realistic surroundings. When the bytes run out every choice takes its first option,
so the generator always terminates and empty input yields the smallest valid program.

When a valid-mode program reports a diagnostic, decide from the grammar which side is wrong. If the
generator wrote invalid syntax, fix the generator; if the syntax is valid, it is a parser bug (§7).

### 5.4 Other suites

`crates/k_syntax/tests/invariants.rs` is the original, minimal statement of the four core invariants
(no panic, lossless tokens, lossless tree, spans in bounds). `crates/k_syntax/tests/robustness.rs`
complements the properties without proptest: a deterministic random stress run over K-shaped fragments
(every invariant above plus message style; in debug builds the parser's fuel assertion also catches loops
that do not consume input), every recursive construct nested past the parser's limit on a bounded stack
(K0134), very deep trees dropped on a 1 MiB stack, and quick fixes (applying the machine-applicable ones
yields code that parses cleanly; the others are marked `maybe-incorrect`). `K_STRESS_CASES=N` raises the
number of random inputs (default 3000).

`crates/k_syntax/tests/adversarial.rs` holds regressions from adversarial testing that the corpus cannot
express: recursion that must be bounded by the nesting limit (function-type chains, `if`/`match` nested
in conditions) and dumps of very deep trees. It is a separate test binary because a stack overflow
aborts the whole binary.

Later stages add their own property files, for example: formatting is idempotent; type checking never
panics on any parseable input; codegen output is deterministic.

---

## 6. Fuzzing

Fuzz targets use [cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html) (libFuzzer with
AddressSanitizer). They need a nightly toolchain, so they live outside the stable workspace in `fuzz/`
(`exclude = ["fuzz"]` in the root `Cargo.toml`).

### 6.1 Setup

```sh
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz
fuzz/seed-corpus.sh            # copy every tests/corpus/**/*.k into fuzz/corpus/<target>/
```

`fuzz/corpus/` and `fuzz/artifacts/` are git-ignored working state. Re-run `seed-corpus.sh` after adding
corpus cases; it keeps what the fuzzer has already found.

### 6.2 Targets

| Target | Input | Asserts |
|---|---|---|
| `lex` | arbitrary bytes, decoded lossily | `check_lexed` and valid lexer diagnostics |
| `parse` | arbitrary UTF-8 (other inputs skipped) | `check_fast`: no panic, `SourceFile` root, valid diagnostics, bounded error count |
| `parse_roundtrip` | arbitrary UTF-8 | `check_all`: the tree text equals the input exactly, token identity, re-lexing, dumps, determinism |
| `structured` | bytes for `kgen::generate` | `check_all`, and no diagnostics at all for valid-mode programs |

All targets share `crates/k_syntax/tests/support/{oracle,kgen}.rs` through `fuzz/src/lib.rs`. Debug
assertions are on in fuzz builds, so the parser's fuel assertion (a loop that does not consume input)
is a crash too.

### 6.3 Running

From the repository root:

```sh
cargo +nightly fuzz list
cargo +nightly fuzz run parse_roundtrip -- -max_total_time=600 -dict=fuzz/k.dict
cargo +nightly fuzz run structured -- -max_total_time=600 -max_len=8192
cargo +nightly fuzz run lex -- -jobs=8 -workers=8 -max_total_time=3600 -dict=fuzz/k.dict
```

Useful libFuzzer flags: `-max_total_time=S` (stop after S seconds), `-max_len=N` (input size cap; 8192 is
plenty for syntax), `-timeout=S` (report an input that runs longer than S seconds; a hang is a bug, since
parsing is linear), `-rss_limit_mb=N`, `-dict=fuzz/k.dict` (keywords, operators and lexer-mode switches,
which helps the byte-level targets form tokens), `-jobs=N -workers=N` (parallel runs),
`-print_final_stats=1`.

Every target must run clean for at least 60 seconds before a syntax change is merged; run them for
longer (10 minutes or more each) after changes to the lexer's modes, recovery or nesting limits. Typical
throughput on an M-series laptop with sanitizers: `lex` about 4,800 inputs/s, `parse` about 2,600,
`parse_roundtrip` about 500 and `structured` about 200.

### 6.4 When a target crashes

libFuzzer stops and writes the input to `fuzz/artifacts/<target>/crash-<hash>` (or `timeout-`, `oom-`).

1. Reproduce: `cargo +nightly fuzz run <target> fuzz/artifacts/<target>/crash-<hash>`.
2. Minimize: `cargo +nightly fuzz tmin <target> fuzz/artifacts/<target>/crash-<hash>`. For
   `structured`, the artifact is generator bytes; the panic message prints the generated K source, which
   is what you minimize (by hand or with the `parse_roundtrip` target).
3. Triage. For `structured`, a valid-mode diagnostic may be a generator bug: check the source against
   [grammar.md](grammar.md) and fix `kgen.rs` if the generator wrote invalid syntax.
4. Record the minimized K source as a corpus regression case **before** fixing the bug (§7), under
   `tests/corpus/<suite>/regressions/fuzz/` (for example `tests/corpus/parser/regressions/fuzz/`).
   Every top-level directory of `tests/corpus` is a suite, so fuzz regressions live inside the suite they
   exercise, not in a separate top-level directory. Write the expectation files by hand from the grammar
   and add a row to the directory's `README.md` naming the target and the symptom.
5. Fix the bug; the corpus case and the fuzz artifact must then pass.

Later stages add targets (`check`, `codegen`, `fmt`) on the same pattern: an oracle module shared with a
stable property test, a byte-level target and a structure-aware one.

---

## 7. Regression tests

Every fixed bug gets a test that fails without the fix:

- For a compiler bug, add a corpus case under `tests/corpus/<suite>/regressions/` (and a row in that
  directory's `README.md`), named
  `<short-description>.k` (for example `regressions/crlf-inside-interpolation.k`). Put a `#` comment at the
  top of the expectation file (`.tokens` or `.diag`), or a `;` comment in `.ast`, explaining the bug.
- For a fuzz or proptest finding, the minimized input becomes the case's source; fuzz findings go in
  `tests/corpus/<suite>/regressions/fuzz/` (§6.4).
- For runtime or server bugs, add the test next to the code it fixes, with a comment naming the bug.

---

## 8. End-to-end tests

- **CLI:** `crates/k_cli/tests/cli.rs` runs the built `k` binary (`env!("CARGO_BIN_EXE_k")`) in
  temporary directories and asserts exit codes (0 = success, 1 = problems found, 2 = usage or internal
  error), stdout/stderr and output formats (`--format json` emits JSON Lines).
- **Projects (planned):** `tests/e2e/<name>/` holds small K projects. Each test runs `k check`, `k test`
  and `k build` on the project and asserts the results.
- **Server (planned):** a built `dist/app` is started on a free port and exercised over HTTP: SSR HTML,
  hydration data, RPC calls (Connect JSON), typed errors, guards, static assets.
- **Browser (planned):** with a Chrome or Chromium executable available, tests drive it through the
  DevTools protocol from Rust to check hydration, navigation and reactivity in a real DOM. They skip with
  a message when no browser is found.
- **Example apps (planned):** each app under `examples/` has its own K tests (`k test`) and is built and
  smoke-tested by the e2e suite.

---

## 9. Benchmarks and performance budgets

Benchmarks use [criterion](https://docs.rs/criterion) and live in `crates/<crate>/benches/`. They report
throughput in bytes per second where that is meaningful, so results compare directly with the budgets in
[architecture.md](architecture.md#12-performance-budgets).

```sh
cargo bench -p k_syntax                              # run
cargo bench -p k_syntax -- --save-baseline main      # record a baseline
cargo bench -p k_syntax -- --baseline main           # compare a change against it
```

`crates/k_syntax/benches/syntax.rs` measures, in lines/s and bytes/s, lexing and parsing of realistic
generated K (`lex`, `parse`: 1k, 10k and 100k lines; `parse_only`: the parser and tree sink alone), of one
typical 300-line file (`typical_file`), and of fifteen pathological families at about 1 MB each
(`pathological_lex`, `pathological_parse`: deep nesting at and past the limit, very long lines and strings,
huge and deeply nested interpolations, an error on every line, comments, non-ASCII text, long member and
operator chains). All input comes from the deterministic generator in `crates/k_syntax/benches/workload/`,
whose output is itself tested (`crates/k_syntax/tests/workload.rs`: realistic input is valid K for many
seeds, covers every declaration kind, and each pathological family has its documented shape).

**Budget check.** `scripts/bench-check.sh` runs the ignored tests in `crates/k_syntax/tests/perf_budget.rs`
in a release build and fails if lexing, parsing or the typical file miss their budgets, if any
pathological family drops below its floor, or if parse time grows faster than the input. Methods,
baselines and history are in [performance.md](performance.md). New hot paths get a benchmark when they are
written. A change that makes a benchmark more than 5% slower needs a stated reason.

---

## 10. Planned suites

These suites are added to `k_corpus` together with the stages they test. Their formats are fixed now so
tests can be written early.

| Suite | Runs | Expectations |
|---|---|---|
| `check` | lex, parse, semantic analysis | `name.diag` (required; an empty `.diag` file asserts a clean check) |
| `codegen` | the full compiler | `name.client.js`, `name.server.js`, `name.css` (each optional; at least one) |
| `fmt` (deferred) | the formatter | `name.formatted.k` |

**Multi-file cases** (for `check` and `codegen`): a directory containing a `k.toml` file is one case
instead of each `.k` file being one. Its expectations are `expected.diag` (and `expected.*.js`,
`expected.css`) inside that directory, and each `.diag` header prefixes its span with the file path
relative to the case directory: `error[K1001] src/shop/Cart.k:3:5-3:9`.
