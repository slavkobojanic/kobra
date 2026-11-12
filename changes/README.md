# Changelog fragments

Every pull request with a user-visible change adds one file here, named after the change
(`grouped-bars.md`), and the release compiles them into `CHANGELOG.md`, the book's changelog page
(`docs/book/src/reference/changelog.md`) and the upgrade guide (`docs/book/src/apps/upgrading.md`).
The format is decided in `docs/design/48-releases-and-versioning.md` ("Changelog fragments"); the
procedure is `docs/compiler/release.md` §3.

```markdown
---
kind: stdlib
design: 37
book: apps/charts.md
---
`BarChart` draws grouped bars: `BarChart(series:)` takes several series.
```

- **`kind`**, in the order the changelog lists them: `breaking`, `deprecation`, `language`, `stdlib`,
  `runtime`, `tooling`, `preview`, `fix`, `docs`, `internal`.
- **`migration`**, on `breaking` and `deprecation` only: the diagnostic code whose machine-applicable fix
  `k upgrade` applies (one of `k_diagnostics::codes::MIGRATIONS`), or `manual`; a manual migration adds
  its section to the upgrade guide, after `<a id="<file name without .md>"></a>`.
- **`book`**, required on `breaking`, `deprecation`, `language`, `stdlib`, `runtime`, `tooling` and
  `preview`: the book pages the pull request changes, comma-separated, or `none: <reason>`. Optional on
  `fix`; absent on `docs` and `internal`.
- **`design`**, optional: the design topic's number.
- **The entry** after the front matter: one or two sentences in the present tense, for users. It is
  published as written.
- **Nothing user-visible?** Add a `kind: internal` fragment with a one-line reason; internal fragments are
  dropped at release.

`_highlights.md` holds the next release's hand-written highlights. Check the fragments with
`cargo test -p xtask` (or `cargo xtask fragments`), and that each one's book pages are in your change
with `cargo xtask docs-gate origin/main`.
