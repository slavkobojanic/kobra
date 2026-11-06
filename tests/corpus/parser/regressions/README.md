# Parser regressions

Each case pins down a parser bug that was found and fixed (see
[docs/compiler/testing.md §7](../../../../docs/compiler/testing.md#7-regression-tests)). The first
comment of the `.ast` file explains the bug.

| Case | Bug |
|---|---|
| `style-combinator-junk-terminates.k` | A style rule of only combinators looped without consuming input |
| `nesting-too-deep.k` | Unbounded recursion on deep nesting; now K0134 and an iterative skip |
| `found-token-at-line-start-in-block.k` | K0122 for a junk run at a line start said "found end of line" instead of naming the token |
| `found-token-at-line-start-in-member-list.k` | The same for K0126 |
| `found-token-at-line-start-at-top-level.k` | The same for K0125 |
| `error-token-after-statement-is-not-a-second-statement.k` | An Error token after a statement also got K0102 with a useless fix |
| `error-token-inside-name.k` | A non-ASCII character in or before a name produced K0100/K0102/K0125 on top of K0001 |
| `error-token-in-type-position.k` | An Error token in a type position also got K0123 at the next token |
| `expected-token-then-junk-is-one-diagnostic.k` | "expected `{`" and the block's junk-run K0122 were both reported at the same span |
| `member-name-error-is-not-a-second-statement.k` | K0100 after `.` or `else` was followed by K0102 at the same token |
| `symbolic-operator-fixes-keep-words-apart.k` | `&&`/`\|\|`/`!` fixes glued words together (`b&&c` → `bandc`, `if!o` → `ifnot o`) |
| `annotation-order-fix-keeps-modifier-apart.k` | The K0121 fix turned `public@Inject let` into `@Inject publiclet` |
| `enum-case-trailing-comma.k` | `case a,` before a line break built an empty case and a fix producing `case case b` |
| `junk-run-skips-strings-whole.k` | A junk run stopped at a keyword inside a string interpolation and parsed a declaration there |
| `interpolation-ended-by-newline-closes-delimiters.k` | A `(`, `[` or `{` left open in an interpolation ended by a line break swallowed the next lines |
| `interpolation-ended-by-crlf-closes-delimiters.k` | The same with CRLF line endings |
| `guard-else-after-try.k` | `guard let x = try g() else { ... }` gave the guard's `else` to the `try` |
| `duplicate-diagnostic-missing-name-before-generics.k` | `enum<` reported K0100 twice at the `<` (missing name, then missing generic parameter) |
| `duplicate-diagnostic-unfinished-nested-blocks.k` | "expected `{`, found end of file" was reported once per unfinished construct at the same gap |
| `important-without-value-has-no-fix.k` | The K0118 fix for `color: !important;` left the empty value `color:;` |
| `element-rule-does-not-cascade.k` | After the K0116 element rule `button { ... }`, the next style rule produced three follow-on errors |
| `trailing-lambda-in-condition-opens-the-block.k` | Pins §13.4: `if xs.filter { x => ... } { }` opens the if's block at the first `{` |

`crates/k_syntax/tests/adversarial.rs` holds the cases from the same hunt that the corpus cannot
express (unbounded recursion through function types and conditions, dumps of very deep trees).
