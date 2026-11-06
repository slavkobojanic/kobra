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
cargo test -p k_sema --test properties          # semantic property tests (soracle, typed generator; §5.5)
PROPTEST_CASES=3000 cargo test --release -p k_sema --test properties   # longer semantic run
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
cargo run -q -p k_cli -- debug sema   path/to/case     # .diag of a sema case (a .k file or a project directory)
cargo run -q -p k_cli -- check        path/to/project  # every diagnostic, human-readable (or --format json)
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
  sema/
    types/memberwise-missing-field.k       ← a single-file case
    types/memberwise-missing-field.diag
    types/memberwise-missing-field.pending ← expected to fail until the check exists
    modules/import-cycle/                   ← a project case: a directory with k.toml
      k.toml
      src/a/A.k
      src/b/B.k
      expected.diag
      expected.pending
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
| `sema` | `k_sema::Analysis` over a whole project | — | `name.diag` (every diagnostic), `name.pending`; projects: `expected.diag`, `expected.pending`; caret annotations in the sources (§3.8) |

**No `.diag` file means "no diagnostics".** Every diagnostic the stage produces must be listed.

Every lexer and parser case also checks losslessness: the token texts (lexer) or the tree text (parser)
must reproduce the source exactly.

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

**File paths.** Any `SPAN` (in a header, a `label` or an `edit`) may be prefixed with a file path and a
colon: `src/shop/Cart.k:3:5-3:9`. Paths contain no spaces. In **multi-file** cases (`sema` projects)
every header names its file, relative to the project root; a label or edit without a path is in the
header's file. In single-file cases paths are omitted, except for places in other files, such as a
label pointing into the standard library: `label <stdlib>/k/core/List.k:12:5-12:9: declared here`.

```text
error[K1101] src/shop/Cart.k:3:8-3:20: `shop.Missing` doesn't exist
  label src/shop/Other.k:1:1-1:5: a similar name is declared here
  fix "Remove the import"
    edit 3:1-4:1 ""
warning[K1105] App.k:2:8-2:21
```

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

### 3.8 The `sema` suite

The `sema` suite checks **everything `k check` reports** for a project, syntax and semantic diagnostics
together, plus assertions about inferred types and resolved names. The design of the stage is
[sema.md](sema.md); every semantic code, with its primary span and fix, is in
[diagnostics.md](diagnostics.md#semantic-codes).

**Two kinds of case:**

- **Single file:** any `name.k` below `tests/corpus/sema` (outside a project directory). It is analysed
  as the only source file of a library project whose `k.toml` is
  `[package]` / `name = "corpus"` / `k = "0.1"`, at the project path `src/corpus/name.k`. So the module
  is `corpus` and the file's declarations are `corpus.X`. A kebab-case file name can't match a public
  declaration, so a case that needs `public` uses a **path directive** on its first line:

  ```k
  // path: src/shop/Cart.k
  public struct Cart {}
  ```

  `// path: App.k` makes the file the app declaration. The directive is an ordinary comment, so it is
  line 1 of the file and counts in spans.
- **Project:** a directory containing `k.toml`. Every file below it is a project file at its relative
  path (`App.k`, `src/shop/Cart.k`, `migrations/0001_init.k`, `routes.lock`), with two exceptions in the
  directory itself: `expected.diag` and `expected.pending` are the case's expectations. The test name
  ends with `/` (`sema/modules/import-cycle/`). Nothing inside a project directory is a separate case.

**Expectations:**

| File | Meaning |
|---|---|
| `name.diag` / `expected.diag` | Every diagnostic, in the `.diag` format of §3.5. Absent means none. In a project every header names its file. Every code must be registered in `crates/k_diagnostics/src/codes.rs` (the case fails otherwise, even when pending). |
| `name.pending` / `expected.pending` | The case is **test-first**: it is expected to fail until the check exists. One line saying what is missing (`type inference (sema.md §9)`); empty is an error. |

**Pending cases** pass while their expectations are unmet, and **fail once everything holds**, with a
message to delete the `.pending` file. They must still parse exactly as intended: their syntax
diagnostics (K0001–K0999) are compared even while pending, so a typo in a test-first source fails at
once. Pending cases are never blessed. When you implement a check, delete the `.pending` files of its
cases and run the suite: each must now pass as a regular case.

**Caret annotations** assert what the position queries answer. They are comments, so they never change
the program; they do count as lines in spans.

```k
func total(items: List<Item>) -> Money {
    let prices = items.map { item => item.price }
    //  ^^^^^^ type: List<Money>
    //           ^^^^^ resolves: param items 1:12
    //                                    ^^^^^ resolves: member corpus.Item.price
    return prices.sum()
}
//@ 1:6-1:11 type: (List<corpus.Item>) -> Money
```

- A **caret line** is `//`, optional spaces, a run of `^`, then `kind: value`. The carets mark columns of
  the **target line**, the nearest line above that is not itself an annotation; the range runs from the
  first caret's column to just past the last caret's. Several annotation lines may share a target.
- An **explicit line** is `//@ SPAN kind: value`, with `SPAN` as in `.diag` files, for ranges that start
  in the first two columns or span several lines.
- The **range must be exactly a node's range**, trivia excluded: an expression, a pattern binding, a
  declaration's name, a member name after `.`, an element or attribute name, a CSS property, a class or
  keyframes name, a token name.
- `value` is `none` to assert that the query answers nothing there.

| Kind | Query | Value format |
|---|---|---|
| `type:` | `Analysis::type_at` | K type syntax ([sema.md §2](sema.md#2-public-api)): prelude types by simple name, others fully qualified: `List<shop.CartItem>`, `String?`, `(Int) async throws k.http.HttpError -> Bool`, `{error}` |
| `resolves:` | `Analysis::resolve_at` | one of the forms below |
| `side:` | `Analysis::side_of` (on a top-level declaration's name) | `server-only`, `client-only`, `shared` |

| `resolves:` value | Meaning |
|---|---|
| `decl shop.cart.CartService` | a top-level declaration (prelude ones by simple name: `decl Int`) |
| `member shop.Cart.add(product:)` | a member: field, function (with labels), case, computed or generated member (`member Page.product`) |
| `local items 3:9` | a local binding, with the line:column of its binding name |
| `param id 2:14` | a function parameter, with the position of its declaration |
| `element button` / `attribute button.type` | a built-in element / one of its attributes |
| `event button.click` / `event shop.Counter.changed` | an event in `.on(name:)` |
| `css padding` | a CSS property |
| `class shop.Summary.muted` / `keyframes shop.Toast.fade` | a class / keyframes of a component's style block |
| `token Spacing.md` | a theme token, as a member of its category |

A malformed annotation (unknown kind, empty value, carets past the end of the target line, carets on
line 1, an explicit span outside the file) fails the case, pending or not.

**Writing a sema case:**

1. Find the rule in [sema.md](sema.md) and its code in [diagnostics.md](diagnostics.md#semantic-codes)
   (primary span and fix). A rule without a code yet is deferred; don't write cases for it.
2. Put the case under the area's directory: `modules/`, `names/`, `types/`, `effects/`, `control-flow/`,
   `components/`, `views/`, `styles/`, `themes/`, `di/`, `routing/`, `rpc/`, `forms/`, `config/`,
   `persistence/`, `jobs/`, `tests/`, `docs/`, `clean/` (programs that must check cleanly).
3. Keep each case to one rule, with the smallest program that shows it. Name it after the behavior:
   `top-level-case-wildcard.k`, `optional-field-may-be-omitted.k`.
4. Write the `.diag` by hand. Cover the error and its near misses: the valid neighbour of an invalid
   program usually belongs in the same file, so the case also proves the rule doesn't over-fire.
5. If the check doesn't exist yet, add the `.pending` file. Run
   `cargo test -p k_corpus --test corpus -- sema/` and `k debug sema <case>` to see the current output.

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

Later stages add their own property files, for example: formatting is idempotent; codegen output is
deterministic.

### 5.5 Semantic properties

`crates/k_sema/tests/properties.rs` states the invariants of semantic analysis over whole projects. Like
the syntax properties, every check lives in one module, `crates/k_sema/tests/support/soracle.rs`, that the
`check*` fuzz targets (§6) include too. A project is a list of `(project path, text)` pairs; diagnostics are
compared in *normal form* (`soracle::Norm`), with files named by path rather than by `FileId`, because file
ids depend on the order files were added.

| Check | Invariant |
|---|---|
| `check_diagnostics` | codes registered; messages one non-empty line; primary spans, labels and fix edits in a known file, in bounds and on char boundaries; no fix edits a standard-library file; the edits of one fix are sorted and disjoint; diagnostics sorted by span and code; `file_diagnostics(f)` is exactly the slice of `diagnostics()` in `f` |
| `check_deterministic` | a fresh analysis of the same inputs, and one with `Options::with_threads(1)`, report exactly the same list |
| `check_order_independent` | adding the files in another order reports the same diagnostics up to file ids |
| `check_incremental` | after each edit of a sequence (splice into a file, replace, remove or add a file, or set unchanged text), the re-check by the same host equals a fresh check of the edited project, and the snapshot taken before the edit is cancelled |
| `check_fixes_add_no_errors` | applying every machine-applicable fix (identical fixes once; overlapping ones skipped, as `rustfix` does) never increases the number of errors of any code in any file |
| `check_queries` | `type_at`, `resolve_at` and `side_of` answer or decline on every token range without panicking |

The inputs:

- **arbitrary text** and lossily decoded bytes, analysed as the only file of a project at a module path, a
  test file, `App.k` and a stray path;
- **syntax-generated projects**: one to three programs of the grammar generator (§5.3) in two modules,
  mostly ill-typed, which reach the checker's recovery paths;
- **well-typed projects** from the typed generator `crates/k_sema/tests/support/tgen.rs`, which must check
  with **zero errors**;
- **mistaken projects**: a well-typed project with one to four fixable mistakes injected
  (`tgen::inject_fixable_mistakes`: duplicate, unused, misspelt or missing imports, `&&`, `||`, `!`, a
  semicolon), for the fix property;
- **the `sema` corpus**: every case is loaded as the corpus suite does, and its machine-applicable fixes
  are applied (`machine_fixes_in_the_corpus_add_no_errors`);
- a fixed list of edge-case projects (no files, empty or broken `k.toml`, self-referential types, …).

**The typed generator** keeps a type environment and generates every expression *for* a type. A project
has a `model` module of public types, one per file named like the file, and a `logic` module of private
enums, structs, functions and top-level constants over one to four files, which import exactly the
`model` types they name. It covers `Int`, `Double`, `Bool`, `String`, `T?`, `List<T>`, enums with and
without payloads, structs with `let`/`var` fields, defaults and methods; literals, memberwise
initializers, qualified and implicit enum cases, arithmetic, comparisons, `and`/`or`/`not`, concatenation,
interpolation, `??`, `.count`, fields, calls, methods and `if` expressions; and `let`/`var`, assignment,
`if`, `while`, `for`, `if let`, `guard let`, exhaustive and `default` `match` over enums and optionals,
void calls and `return`. It avoids what is ill-typed by design (`==` on enums and structs, implicit
members and `nil` without an expected type, shadowing). `typed_generator_covers_its_constructs` fails if a
construct stops being generated. When a well-typed project reports an error, decide from
[08-types.md](../design/08-types.md) and [sema.md](sema.md) which side is wrong: fix `tgen.rs` if it
wrote an ill-typed program; otherwise it is a checker finding (below).

**Known findings.** A checker bug found by these properties is reported, pinned and *not* worked around
silently:

- a wrong error on a well-typed program becomes a pending corpus case under
  `tests/corpus/sema/regressions/` (§7), and the typed generator steers around exactly that shape, citing
  the case in `tgen::KNOWN_FINDINGS`;
- a machine-applicable fix that adds errors is listed in `KNOWN_BAD_FIXES` in `properties.rs`. Like a
  `.pending` file, a listed corpus case must keep failing; once its fix is repaired the test fails and asks
  for the entry to be removed.

When a finding is fixed, delete the `.pending` file, the `KNOWN_*` entry and the workaround together.

Default case counts are lower than the syntax properties' (256 for arbitrary text, 128 for project
properties) because each case runs several whole-project analyses; the whole file takes about 15 s in a
debug build. `PROPTEST_CASES=N` sets every property's count.

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
| `check` | first byte picks the path (module file, test file, `App.k`, stray file); the rest is UTF-8 | semantic analysis of a one-file project: no panic, `check_diagnostics`, `check_queries` |
| `check_structured` | bytes for one to three `kgen` programs in two modules, then edits | `check_diagnostics`, `check_deterministic`, `check_order_independent`, `check_incremental` |
| `check_typed` | bytes for `tgen::generate`, then edits and mistakes | zero errors, `check_order_independent`, `check_incremental`, `check_fixes_add_no_errors` |

All targets share `crates/k_syntax/tests/support/{oracle,kgen}.rs` and
`crates/k_sema/tests/support/{soracle,tgen}.rs` through `fuzz/src/lib.rs`. Debug assertions are on in fuzz
builds, so the parser's fuel assertion (a loop that does not consume input) is a crash too.

### 6.3 Running

From the repository root:

```sh
cargo +nightly fuzz list
cargo +nightly fuzz run parse_roundtrip -- -max_total_time=600 -dict=fuzz/k.dict
cargo +nightly fuzz run structured -- -max_total_time=600 -max_len=8192
cargo +nightly fuzz run lex -- -jobs=8 -workers=8 -max_total_time=3600 -dict=fuzz/k.dict
cargo +nightly fuzz run check -- -max_total_time=600 -dict=fuzz/k.dict -timeout=20
cargo +nightly fuzz run check_typed -- -max_total_time=600 -max_len=8192 -timeout=20
```

Useful libFuzzer flags: `-max_total_time=S` (stop after S seconds), `-max_len=N` (input size cap; 8192 is
plenty for syntax), `-timeout=S` (report an input that runs longer than S seconds; a hang is a bug, since
parsing is linear), `-rss_limit_mb=N`, `-dict=fuzz/k.dict` (keywords, operators and lexer-mode switches,
which helps the byte-level targets form tokens), `-jobs=N -workers=N` (parallel runs),
`-print_final_stats=1`.

Every syntax target must run clean for at least 60 seconds before a syntax change is merged, and every
`check*` target before a checker change is merged; run them for
longer (10 minutes or more each) after changes to the lexer's modes, recovery or nesting limits. Typical
throughput on an M-series laptop with sanitizers: `lex` about 4,800 inputs/s, `parse` about 2,600,
`parse_roundtrip` about 500 and `structured` about 200. The semantic targets are much slower because
every input is one or more whole-project analyses, standard library included: `check` about 100–150
inputs/s, `check_structured` and `check_typed` about 10–20 (several analyses per input). Replaying the
seeded corpus alone takes them about four minutes, so run them with `-fork=N` (or `-jobs`) and for at
least 10 minutes when their oracle or generator changes.

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

For the `check*` targets, step 3 and 4 differ:

- **Triage.** For `check_typed`, an error on the generated project is triaged like a failing
  well-typed property (§5.5): a generator bug is fixed in `tgen.rs`; anything else is a checker finding.
  A failed fix, order or incremental property is always a checker finding (or an oracle bug in
  `soracle.rs`). The panic message prints every file of the project, which is what you minimize.
- **Pin.** Record the minimized project as a case under `tests/corpus/sema/regressions/fuzz/` with its
  expected `.diag` written by hand, a `.pending` file naming what is missing, and a row in that
  directory's `README.md` (target and symptom). A pending case passes while the checker is wrong and fails
  once it is fixed, so the suite stays green and the finding cannot be forgotten. If the finding keeps
  the target from exploring further, the generator or mistake injector steers around exactly that shape,
  citing the case in `tgen::KNOWN_FINDINGS`.
- **Panics.** A case that panics the checker fails even when pending (the corpus harness reports a panic
  as a failure), so a panic is pinned in the `README.md` row with its minimized source and turned into a
  case by the commit that fixes it.

Later stages add targets (`codegen`, `fmt`) on the same pattern: an oracle module shared with a
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
- A finding that is reported before it is fixed (a checker finding of the semantic properties or fuzz
  targets, §5.5 and §6.4) is pinned the same way with a `.pending` file, so it fails once fixed. The
  `KNOWN_*` lists that steer the generators around it are emptied by the same commit that deletes the
  `.pending` file.
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
cargo bench -p k_sema                                # the semantic benchmarks
```

`crates/k_syntax/benches/syntax.rs` measures, in lines/s and bytes/s, lexing and parsing of realistic
generated K (`lex`, `parse`: 1k, 10k and 100k lines; `parse_only`: the parser and tree sink alone), of one
typical 300-line file (`typical_file`), and of fifteen pathological families at about 1 MB each
(`pathological_lex`, `pathological_parse`: deep nesting at and past the limit, very long lines and strings,
huge and deeply nested interpolations, an error on every line, comments, non-ASCII text, long member and
operator chains). All input comes from the deterministic generator in `crates/k_syntax/benches/workload/`,
whose output is itself tested (`crates/k_syntax/tests/workload.rs`: realistic input is valid K for many
seeds, covers every declaration kind, and each pathological family has its documented shape).

`crates/k_sema/benches/sema.rs` measures semantic analysis (`AnalysisHost` + `diagnostics()`, parsing
included) of generated multi-module projects: a cold check of 1k, 10k and 100k lines on one thread
(`cold_1t`) and on every core (`cold_mt`), a warm re-check after a function body edit (`warm_body`) and
after a public signature edit (`warm_signature`), and four pathological families (`pathological`: deep
generic nesting, a 2,000-case `match`, a 1,000-module import chain, a 1,000-service DI graph). Its
generator, `crates/k_sema/benches/project/`, is tested in `crates/k_sema/tests/project_workload.rs`: every
project, family and benchmark edit analyses with zero diagnostics, sizes are exact and generation is
deterministic.

**Budget check.** `scripts/bench-check.sh` runs the ignored tests in `crates/k_syntax/tests/perf_budget.rs`
and `crates/k_sema/tests/perf_budget.rs` in a release build and fails if lexing, parsing or the typical
file miss their budgets, if any pathological family drops below its floor, if parse time grows faster
than the input, if a cold or warm check of the 100k-line project misses its time budget or its memory
budget, or if check time grows faster than allowed. Methods, baselines and history are in
[performance.md](performance.md). New hot paths get a benchmark when they are
written. A change that makes a benchmark more than 5% slower needs a stated reason.

---

## 10. Planned suites

These suites are added to `k_corpus` together with the stages they test. Their formats are fixed now so
tests can be written early.

| Suite | Runs | Expectations |
|---|---|---|
| `codegen` | the full compiler | `name.client.js`, `name.server.js`, `name.css` (each optional; at least one) |
| `fmt` (deferred) | the formatter | `name.formatted.k` |

The semantic suite planned here as `check` exists as `sema` (§3.8). `codegen` uses the same case kinds:
single files and project directories with `k.toml`, whose expectations are `expected.*` inside the
directory, with paths in `.diag` headers as in §3.5.
