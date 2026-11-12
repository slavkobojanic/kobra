# Formatter cases

Golden cases of `k fmt`'s core, the `k_fmt` crate (`docs/design/56-formatter.md`). Every `name.k`
here formats to the sibling `name.formatted.k`, and every `name.formatted.k` is already formatted.

```sh
cargo test -p k_fmt --test it -- golden::
K_BLESS=1 cargo test -p k_fmt --test it -- golden::   # rewrite the expectations
```

| Directory | Rules |
|---|---|
| `layout/` | indentation, spaces between tokens, blank lines, line endings, joined lines |
| `views/` | the view layout of `docs/design/05-views.md`: blank lines between view nodes, chained modifiers |
| `styles/` | style blocks: declarations, selectors kept as written, `;` after the last declaration |
| `rewrites/` | the canonical rewrites: redundant modifiers, `case _`, trailing commas, sorted imports, `init()` placement |
| `comments/` | every kind of comment, kept and re-indented |

The properties (idempotence, the same syntax tree, the same comments and the same diagnostics) are
checked over these files too, and over the whole corpus, the examples and the book's samples:
`cargo test -p k_fmt --test it -- properties::`.
