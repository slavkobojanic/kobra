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
book.toml              mdBook configuration
src/SUMMARY.md         the table of contents; every chapter must be listed here
src/                   chapters, grouped by part (getting-started, language, apps, reference)
theme/k-highlight.js   the highlight.js language definition for K (```k code blocks)
theme/k.css            small style additions
```

## Writing chapters

- Tag K code blocks with `k`, shell commands with `bash`, TOML with `toml`, and everything else
  (directory trees, compiler output, Dockerfiles) with `text`. Only languages bundled with mdBook's
  highlight.js, plus `k`, are highlighted.
- Every K sample must follow the design: `let`/`var`, `func`, named required arguments and chained
  options, K element names, typed classes, camelCase style properties, and the layering rules.
- Mark a feature the design explicitly defers with a `> **Planned.**` note, and a feature that comes from
  a design proposal with a `> **Proposal.**` note naming the file in `docs/design/proposals/`. Keep
  [`src/reference/design-status.md`](src/reference/design-status.md) in step with both.
- Link between chapters with relative `.md` links; mdBook rewrites them to `.html`. Refer to files outside
  the book, such as proposals, by their repository path in backticks, because the site doesn't contain
  them.

## Syntax highlighting

mdBook ships highlight.js 10 and highlights code blocks before additional scripts load. Blocks tagged
`k` are left untouched at that point, since the language is unknown. `theme/k-highlight.js` then
registers K (keywords from `docs/compiler/grammar.md`, string interpolation, unit and color literals,
annotations, types and implicit members) and highlights those blocks. When the grammar's keyword lists
change, update the lists at the top of that file.
