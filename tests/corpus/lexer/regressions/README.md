# Lexer regressions

Each case pins down a lexer bug that was found (see
[docs/compiler/testing.md §7](../../../../docs/compiler/testing.md#7-regression-tests)). The first
comment of the `.tokens` file explains the bug.

| Case | Bug |
|---|---|
| `style-block-in-string-interpolation-ends-at-newline.k` | A `style {` inside a string interpolation survived the line end and lexed the rest of the file in style mode |
| `style-value-interpolation-in-string-ends-at-newline.k` | The same with a style-value interpolation open on top of the style frame |
| `style-block-in-string-interpolation-ends-at-crlf.k` | The same with a nested string and CRLF line endings |
| `element-rule-ends-its-statement.k` | The `;` inside `button { ... }` (RA-8) did not end the style statement, so the next rule lexed as a declaration |
