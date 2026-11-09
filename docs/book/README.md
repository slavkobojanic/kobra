# The K Programming Language (book)

This directory holds the source of the K language guide, built with
[mdBook](https://rust-lang.github.io/mdBook/).

The language design in [`docs/design`](../design/README.md) is the source of truth. The book explains
it for people learning K; where they disagree, the design wins and the book should be fixed.

## Building

Install mdBook once (this only touches `~/.cargo`):

```sh
. "$HOME/.cargo/env"
cargo install mdbook --locked
```

Build the site from the repository root:

```sh
mdbook build docs/book
```

The output goes to `docs/book/book/` (git-ignored). Open `docs/book/book/index.html` in a browser.

The build must finish with no warnings. `create-missing = false` in `book.toml` makes a chapter that is
listed in `src/SUMMARY.md` but missing on disk an error instead of a silently created empty page.

## Serving while editing

```sh
mdbook serve docs/book --open
```

`mdbook serve` rebuilds on every save and reloads the browser. It listens on `http://localhost:3000`;
pass `--port` to change it.

## Layout

```text
book.toml                    mdBook configuration
src/SUMMARY.md               the table of contents; every chapter must be listed here
src/                         chapters, grouped by part (getting-started, language, apps, examples, reference)
src/reference/stdlib/        generated: the standard library reference, one page per module
src/reference/diagnostics.md generated: every diagnostic code
check/BookTheme.k            the theme the samples are checked against
check/diagnostic-fixes.txt   the fix for each diagnostic code
check/stale.txt              phrases that must not appear in prose (stale claims, removed syntax)
check/inline-names.txt       inline code that looks like a K name but deliberately isn't one
check/audit.toml             the page-audit ledger: the commit each page was last audited at
theme/k-highlight.js         the highlight.js language definition for K (```k code blocks)
theme/k.css                  small style additions
```

## Writing chapters

- Tag K code blocks with `k`, shell commands with `bash`, TOML with `toml`, and everything else
  (directory trees, compiler output, Dockerfiles) with `text`. Only languages bundled with mdBook's
  highlight.js, plus `k`, are highlighted.
- Every K sample must follow the design: `let`/`var`, `func`, named required arguments and chained
  options, K element names, typed classes, camelCase style properties, and the layering rules.
- Mark a feature that the first compiler releases defer with a `> **Planned** (id).` note, or
  `(**Planned** id)` inline, where `id` is the feature's row in `crates/k_sema/src/features.rs` (add one
  if it's new; several ids are written `(a, b)`). Mark a question the design records as still open with a
  `> **Open question.**` note naming the design file, and keep the Open questions table of
  [`src/reference/design-status.md`](src/reference/design-status.md) in step. Its Planned table is
  generated from the notes (below).
- A command line for a planned command, in a `bash` or `console` block, ends with `# planned`.
- Write a version as `{{k_version}}` where it should follow the release (`k = "{{k_version}}"`); the
  site build and the snippet test replace it with the current minor.
- Members of a public type are public by default, so samples never write `public` on a field, function,
  static member or `init`. Only top-level declarations (and `public extend`) carry `public`.
- Link between chapters with relative `.md` links; mdBook rewrites them to `.html`. Refer to files outside
  the book, such as design topics, by their repository path in backticks, because the site doesn't contain
  them.

## Every K sample compiles

`cargo test -p k_sema --test it -- book::` extracts every fenced block tagged `k` from `src/` and checks it with
the same analysis `k check` runs. A block passes when it reports no error. The rules
(`crates/k_sema/tests/book.rs` has the details):

| Block | What the test does |
|---|---|
| ` ```k ` | checks it; it must compile |
| ` ```k,ignore ` | skips it: a fragment, pseudo-code or a Planned feature |
| ` ```k,fails=K2011 ` | checks that it reports that code (`+` joins several), and no other error: for samples that show a mistake |
| ` ```k,standalone ` | checks it without the chapter's project files, for a sample that redeclares one of them |

- **Hidden lines.** A line starting with `~` is hidden in the book (`output.html.code.hidelines` in
  `book.toml`; readers can reveal it) but included in the check. Use hidden lines for imports and for the
  declarations around a fragment, such as a `component` wrapped around a view snippet. The `~` is removed
  and the rest of the line is kept, so write `~    view {` with the indentation after the `~`.
- **Project files.** A block whose first visible line is `// src/<module>/<File>.k` (or `// App.k`) is
  that file of the chapter's project. All of a chapter's project files are checked together, and every
  other block of the chapter is checked with them, in the module of the first one.
- **Chapter context.** A fenced block inside an HTML comment (`<!-- … -->`) is invisible in the book but
  still checked. A chapter declares the types its samples use in such a block, as a project file.
- **Excerpts.** A block whose first visible line is `// examples/…/File.k` or
  `// crates/k_sema/stdlib/…/File.k` is an excerpt: each run of lines between `// ...` lines must occur,
  line by line (indentation aside), in that file. The examples are compiled by their own tests, so an
  excerpt is checked by being in the file. Prefer excerpts in the example chapters.
- **The theme.** A project without a `theme` gets `check/BookTheme.k`, which declares the tokens the
  samples use (`.md`, `.muted`, `.lg`).

## Prose checks

The book's prose is checked too, by `cargo test -p k_sema --test it -- book:: book_links::` and
`cargo test -p k_cli --test it -- book_commands:: version::`:

- every relative link and `#anchor` resolves (in `docs/design` and `docs/compiler` as well), and a
  chapter links only inside the book;
- every `> **Planned** (id).` note names a planned row of `k_sema::features`, and every planned row is
  named by a note. The Planned table of `reference/design-status.md` is generated from the notes:
  `K_BLESS=1 cargo test -p k_sema --test it -- book::` rewrites it;
- no pattern of `check/stale.txt` (one regular expression per line, then ` # ` and the reason) occurs in
  prose or in `text`, `bash`, `console` and `toml` blocks. When a change makes a phrase false or removes
  syntax, add the old form there;
- inline code that reads as a qualified K name or an annotation (`RemoteClient.at`, `k.auth.Session`,
  `@Load`) resolves in the standard library; `check/inline-names.txt` lists the deliberate exceptions;
- a sentence that says a diagnostic has a quick fix names its code ("a compile error (`K2304`) with a
  quick fix to `default`"), and a corpus `.diag` case pins that code's fix (a `fix` line);
- every page of `SUMMARY.md` has an entry in `check/audit.toml`, the page-audit ledger: audit a new page
  when you add it (`docs/compiler/documentation.md` §5);
- every `k …` command line in `bash`, `sh` and `console` blocks, and inline `` `k …` `` code, parses with
  the real command-line definitions, and the toolchain version is the current one everywhere it's
  written.

## Generated reference pages

`src/reference/diagnostics.md` and `src/reference/stdlib/*.md` are generated, and
`cargo test -p k_corpus --test it` fails when they are out of date:

- the diagnostics page from the code registry (`crates/k_diagnostics/src/codes.rs`), an example per code
  from the golden corpus (`tests/corpus`) with the compiler's actual output, and the fixes in
  `check/diagnostic-fixes.txt` (one line per code; the test fails when a code has none);
- one page per standard-library module, from the `///` comments in `crates/k_sema/stdlib`.

Regenerate both after changing any of their sources:

```sh
K_BLESS=1 cargo test -p k_corpus --test it
```

A new stdlib module also needs an entry in `src/SUMMARY.md`; the test says which.

## Edit links

`book.toml` leaves `edit-url-template` and `git-repository-url` unset until the repository has a public
URL. Set them when building for publishing, for example:

```sh
MDBOOK_OUTPUT__HTML__EDIT_URL_TEMPLATE='https://<host>/<repo>/edit/main/docs/book/src/{path}' \
MDBOOK_OUTPUT__HTML__GIT_REPOSITORY_URL='https://<host>/<repo>' \
mdbook build docs/book
```

## Syntax highlighting

mdBook ships highlight.js 10 and highlights code blocks before additional scripts load. Blocks tagged
`k` are left untouched at that point, since the language is unknown. `theme/k-highlight.js` then
registers K (keywords from `docs/compiler/grammar.md`, string interpolation, unit and color literals,
annotations, types and implicit members) and highlights those blocks. When the grammar's keyword lists
change, update the lists at the top of that file.
