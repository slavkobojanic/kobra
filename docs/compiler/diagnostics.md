# Diagnostics

Every problem the K toolchain reports about user input is a structured diagnostic. This document lists the
rules all diagnostics follow and every registered code with its **primary span convention**: exactly
which source text the primary span covers. The conventions are normative, because `.diag` golden files
(see [testing.md](testing.md)) assert spans exactly.

The code registry in `crates/k_diagnostics/src/codes.rs` is the source of truth for codes, names and
default severities; this document must be kept in sync with it.

## Rules

1. **Never panic.** No input, however malformed, may crash the compiler. Problems become diagnostics, and
   every stage recovers and continues so a single run reports as much as possible.
2. **Structure.** A diagnostic has a code (`K0002`), a severity (`error` or `warning`), a message, one
   primary span, optional secondary labels, notes and help lines, and optional quick fixes.
3. **Messages** are one line, start lowercase, have no trailing period, and quote code in backticks:
   ``expected `)`, found `,` ``.
4. **The primary span is the smallest text that identifies the problem.** When something is *missing*,
   the span is empty (an insertion point) and sits immediately after the last token before the gap,
   so that a fix inserting text there produces correctly placed code.
5. **Quick fixes** are `machine-applicable` only when applying them is certainly right and produces valid
   code; otherwise they are `maybe-incorrect`. Several alternative fixes are listed most likely first.
6. **Codes are stable.** A code keeps its meaning forever and is never reused. New problems get new codes
   in their stage's range.
7. **Ordering.** Diagnostics are reported sorted by file, then span start, then span end, then code.

## Code ranges

| Range | Stage |
|---|---|
| K0001–K0099 | Lexer |
| K0100–K0499 | Parser |
| K1000–K1999 | Modules, imports, name resolution, visibility |
| K2000–K2999 | Types, expressions, control flow, errors |
| K3000–K3999 | Components, views, styles, themes, reactivity |
| K4000–K4999 | Services, dependency injection, layering, server/client boundary |
| K5000–K5499 | Pages and routing |
| K5500–K5999 | Protocols and RPC |
| K6000–K6999 | Persistence |
| K7000–K7499 | Forms and validation |
| K7500–K7999 | Tests |
| K8000–K8999 | Lockfiles and packages |
| K9000–K9999 | Driver, I/O and internal errors |

## Lexer codes

| Code | Name | Primary span | Fix |
|---|---|---|---|
| K0001 | `unexpected-character` | The maximal run of consecutive unexpected characters (one `Error` token). A help line suggests `bind x` for `$x`. | — |
| K0002 | `unterminated-string` | The opening `"` of the string (1 character). | — |
| K0003 | `unterminated-block-comment` | The opening `/*` of the outermost unterminated comment (2 characters). | — |
| K0004 | `invalid-escape` | The escape: `\` and the character after it, or just `\` at the end of a line or file. | — |
| K0005 | `invalid-color-literal` | The whole `ColorLit` token, including `#`. | — |
| K0006 | `invalid-number-literal` | The whole number token. | — |
| K0007 | `brace-escape` | The two characters `\{`. | machine-applicable: replace with `{{` |
| K0008 | `invalid-unicode-escape` | The whole `StringEscape` token starting with `\u`. | — |

Suggested messages: K0001 ``unexpected character `$` `` (or `unexpected characters` for a run); K0002
`unterminated string literal`; K0003 `unterminated block comment`; K0004 ``unknown escape `\q` ``; K0005
``invalid color literal `#12345` ``; K0006 `invalid number literal`; K0007 ```\{` is not an escape; use
`{{` ``; K0008 `invalid unicode escape`.

## Parser codes

"Found token" means the first non-trivia token where the parser detected the problem. "Gap" means an
empty span immediately after the previous non-trivia token (rule 4).

| Code | Name | Primary span | Fix |
|---|---|---|---|
| K0100 | `expected-token` | The found token; the gap if the found token is on a later line or is the end of file. | — |
| K0101 | `unclosed-delimiter` | The opening `(`, `[` or `{` (including a style block's `{`). | — |
| K0102 | `statements-on-one-line` | The first token of the second statement, member or item; when that element is a string literal, the whole string (`"b"` in `"a" "b"`). | machine-applicable: replace the whitespace before it with a newline and the indentation of the line |
| K0103 | `semicolon` | The `;`. | machine-applicable: delete it (replace it and following whitespace with a newline and indentation if a statement follows on the same line) |
| K0104 | `symbolic-and` | The `&&`. | machine-applicable: replace with `and`, adding a space on each side that touches a word character (`b&&c` → `b and c`) |
| K0105 | `symbolic-or` | The `\|\|`. | machine-applicable: replace with `or`, spaced as for K0104 |
| K0106 | `symbolic-not` | The `!`. | machine-applicable: replace `!` and any whitespace after it with `not `, with a space before it if a word character precedes the `!` (`if!a` → `if not a`) |
| K0107 | `ternary` | The `?` of the ternary. | — (help: use `if c { a } else { b }`) |
| K0108 | `reserved-word` | The reserved keyword token. | — |
| K0109 | `misplaced-when` | The `when` token. | — |
| K0110 | `misplaced-bind` | The `bind` token. | — |
| K0111 | `import-after-declaration` | The whole `Import` node. Label: the first item, "first declaration is here". | — |
| K0112 | `unknown-unit` | The whole `UnitLit` token. | maybe-incorrect: the known unit within edit distance 1 of the suffix, if exactly one exists. For an upper-case base prefix whose digits are valid for the base (`0X1F`, `0B101`, `0O17`), the message is ``` `0X` is not a number prefix; write `0x` ``` and the machine-applicable fix lower-cases the letter |
| K0113 | `non-associative-chain` | The second operator token of the chain. | — |
| K0114 | `misplaced-try` | The `try` or `await` token. | maybe-incorrect: wrap the operand from `try`/`await` to the end of the expression in parentheses |
| K0115 | `bare-identifier-pattern` | The identifier. | maybe-incorrect: `let name`; maybe-incorrect: `.name` |
| K0116 | `type-selector` | The element name. | — |
| K0117 | `missing-semicolon` | The gap after the declaration's last value token. | machine-applicable: insert `;` |
| K0118 | `important-not-supported` | From `!` through `important`. | machine-applicable: delete it and the whitespace before it; not offered when `!important` is the whole value (`color: !important;`), since deleting it would leave an empty value |
| K0119 | `invalid-range-operator` | The `..`. | maybe-incorrect: `..<`; maybe-incorrect: `...` |
| K0120 | `nested-optional` | From the second `?` through the last `?` on the line (`T??` → the second `?`; `T? ? ?` → `? ?`). One diagnostic per type, however many extra `?` follow. | machine-applicable: delete it, leaving `T?` |
| K0121 | `annotation-order` | The misplaced `Annotation` node. | machine-applicable: move it before the first modifier (deleting the whitespace after it only when whitespace also precedes it, so `public@Inject let` keeps `public let` apart) |
| K0122 | `expected-expression` | As K0100. | — |
| K0123 | `expected-type` | As K0100. | — |
| K0124 | `expected-pattern` | As K0100. | — |
| K0125 | `expected-declaration` | The first token of the run the parser skipped at the top level. | — |
| K0126 | `expected-member` | The first token of the run the parser skipped in a member list. | — |
| K0127 | `duplicate-modifier` | The second modifier token. | machine-applicable: delete it |
| K0128 | `empty-interpolation` | The `{}` (2 characters). | in strings, machine-applicable: replace with `{{}` |
| K0129 | `throws-without-type` | The `throws` token. | — |
| K0130 | `multiple-enum-cases` | The first `,` after the case name. | machine-applicable: put each case on its own line with its own `case`; for a trailing comma with no name after it (`case a,` then a line break), "Remove the trailing comma" deletes it |
| K0131 | `top-level-var` | The `var` token. | maybe-incorrect: replace with `let` |
| K0132 | `id-selector` | The `ColorLit`-looking `#name` token in the selector. | — |
| K0133 | `declaration-outside-rule` | The whole `Declaration` node, including its `;`. | — |
| K0134 | `nesting-too-deep` | The first token beyond the nesting limit of 400 levels (grammar.md RA-34). Reported once per over-deep construct. | — |

When one token triggers both a lexer and a parser diagnostic, both are reported (for example `#zz` in a
selector reports K0005 and K0132). The parser never adds a diagnostic for a lexer `Error` token (K0001
covers it; this includes K0102 and an "expected a type/name" at the token after it), and when an
enclosing construct would report K0100, K0122, K0123 or K0124 at exactly the span of the parser
diagnostic just reported (for example a block's junk-run K0122 after a statement's "expected `{`"), it
does not: one gap gets one diagnostic. K0102 is not reported at a token that already has a parser
diagnostic (`x.5`, `else b`): the line break it would insert fixes nothing. When K0134 skips to the
end of the file, K0101 is not reported for the delimiters that enclose the over-deep construct.
Finally, no two diagnostics of one parse have the same code and the same primary span: after sorting,
of each such run only the first reported is kept (`enum<` reports "expected a name, found `<`" once,
not also "expected a generic parameter name" at the same `<`).

Messages that name the found token (``found `and` ``) say `end of line` or `end of file` only when the
primary span is a gap; a junk run that starts at the beginning of a line names its first token.

Suggested parser messages: K0100 ``expected `)`, found `,` `` (the found part is `end of line` or
`end of file` when the span is a gap); K0101 ``unclosed delimiter `(` ``; K0102 `expected a line break
before this`; K0103 `semicolons are not used in K; end the statement with a line break`; K0104 ``` `&&` is
not an operator; write `and` ```; K0112 ``unknown unit `pz` ``; K0134 `nesting exceeds the limit of 400
levels`.

## Driver codes

| Code | Name | Primary span | Fix |
|---|---|---|---|
| K9001 | `invalid-utf8` | Empty span at the byte offset of the first invalid byte. | — |
| K9002 | `io-error` | Empty span at offset 0 of a placeholder file named after the path. | — |

## Adding a code

1. Add the constant to the `codes!` table in `crates/k_diagnostics/src/codes.rs`, in numeric order, with a
   kebab-case name, default severity and a one-sentence summary.
2. Add a row here with its primary span convention and fixes.
3. Add corpus cases (`.diag`) that produce it, including a case for each fix.
