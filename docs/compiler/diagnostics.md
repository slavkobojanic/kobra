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
| K1000–K1999 | Modules, imports, name resolution, visibility, doc comments |
| K2000–K2999 | Types, expressions, effects, control flow, errors |
| K3000–K3999 | Components, views, styles, themes, reactivity |
| K4000–K4999 | Services, dependency injection, layering, server/client boundary |
| K5000–K5499 | Pages and routing |
| K5500–K5999 | Protocols and RPC |
| K6000–K6999 | Persistence |
| K7000–K7499 | Forms and validation |
| K7500–K7999 | Tests |
| K8000–K8999 | Lockfiles, manifest and packages |
| K9000–K9999 | Driver, I/O and internal errors |

The semantic ranges are split into sub-ranges per area of [sema.md](sema.md); the headings of
[Semantic codes](#semantic-codes) name them. A new code goes into its area's sub-range, after the last
code there.

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

## Semantic codes

Reported by `k_sema` ([sema.md](sema.md)). These are the **v0** codes: every check that sema.md marks
v0 reports one of them. Deferred checks get codes when they are implemented. Unless a row says
otherwise, a diagnostic has no labels, and "as K2003" means the same span convention as that code.
Fixes are machine-applicable only where the row says so.

The `sema` corpus suite ([testing.md §3.8](testing.md#38-the-sema-suite)) holds cases for these codes;
cases for checks that don't exist yet are marked pending.

### Project layout and files (K1000–K1099)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K1001 | `stray-source-file` | error | Empty span at offset 0 of the file. | — |
| K1002 | `public-name-mismatch` | error | The declaration's name token. | Rename the declaration to the file's name (maybe-incorrect). |
| K1003 | `multiple-public-declarations` | error | The name token of the second and every later public declaration; label: the first. | Remove `public` (maybe-incorrect). |
| K1004 | `duplicate-declaration` | error | The later declaration's name token (by path, then offset); label: the first. | — |
| K1005 | `misplaced-declaration` | error | The declaration's keyword token. | — |

### Imports and the module graph (K1100–K1199)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K1101 | `unresolved-import` | error | The import's path, from its first segment to its last. | ``Replace with `closest.Name` `` (maybe-incorrect). |
| K1102 | `import-not-visible` | error | The last segment of the import's path; label: the declaration. | — |
| K1103 | `import-cycle` | error | In each module of the cycle, the path of the import that continues it; labels: the other imports of the cycle, in order. | — |
| K1104 | `test-only-import` | error | The import's path. | — |
| K1105 | `unused-import` | warning | The import's path. | Remove the import (machine-applicable). |
| K1106 | `duplicate-import` | error | The later import's path; label: the earlier one, or ``declared in this module here`` at the module's own declaration of the name. | Remove the duplicate when both name the same declaration, including an import of the module's own declaration (machine-applicable). |

### Name resolution (K1200–K1299)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K1201 | `unresolved-name` | error | The name token. | Import the declaration when exactly one visible declaration has the name (machine-applicable); otherwise the closest name (maybe-incorrect). |
| K1202 | `unresolved-type` | error | The type's path. | As K1201. |
| K1203 | `unknown-member` | error | The member name token after `.`. | ``Replace with `name` ``, the closest member (maybe-incorrect). |

### Visibility (K1300–K1399)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K1301 | `not-visible` | error | The name token of the use; label: the declaration. | — |
| K1302 | `redundant-public-member` | warning | The `public` token. | Remove `public` and the space after it (machine-applicable). |
| K1303 | `public-inject` | error | The `public` token. | Remove `public` (machine-applicable). |
| K1304 | `private-extend` | error | The `private` token. | Remove `private` (maybe-incorrect). |
| K1305 | `conflicting-extension-member` | error | The second member's name token; label: the first. | — |

### Doc comments (K1900–K1999)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K1901 | `unresolved-doc-link` | warning | The link's name inside the brackets and backticks. | — |

### Types and expressions (K2000–K2099)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K2001 | `type-mismatch` | error | The expression; label: where the expected type comes from, when it is written in source. | Conversions the design names (`String(n)`, `.formatted()`) (maybe-incorrect). |
| K2002 | `cannot-infer` | error | The expression (`[]`, `nil`, a lambda). | — |
| K2003 | `missing-argument` | error | Empty span just before the call's `)`, or after the callee when it has no argument list. | Insert `label: <placeholder>` (maybe-incorrect). |
| K2004 | `unknown-argument-label` | error | The label token. | The closest label (maybe-incorrect). |
| K2005 | `not-callable` | error | The callee expression. | — |
| K2006 | `unknown-implicit-member` | error | The name after `.`. | ``Replace with `name` ``, the closest member of the expected type (maybe-incorrect). |
| K2007 | `implicit-member-without-type` | error | The whole implicit member expression, from `.` to the name. | — |
| K2008 | `missing-memberwise-field` | error | As K2003; one diagnostic lists every missing field. | Insert the missing labels with placeholders (maybe-incorrect). |
| K2009 | `unsatisfied-constraint` | error | The type argument, or the argument expression it was inferred from. | — |
| K2010 | `missing-interface-member` | error | The interface's name in the `implements` clause. | Insert stubs (maybe-incorrect). |
| K2011 | `assignment-to-let` | error | The assigned place (left side of `=`, or the receiver of a mutating call); label: the `let`. | Change `let` to `var` when the binding is local (maybe-incorrect). |
| K2012 | `bool-literal-comparison` | warning | The comparison expression. | `x` or `not x` (machine-applicable). |
| K2013 | `invalid-operands` | error | The binary or prefix expression. Covers `==`/`!=` without `Equatable` or across types, `<` without `Comparable`, arithmetic on non-numbers, `Int` with `Double` (no implicit conversions), a length times a length, and `-` on a `Bool`. Operands of `and`/`or`/`not` that aren't `Bool` are K2001 at the operand instead; arithmetic across unit dimensions is K2101. | — |
| K2014 | `not-an-interface` | error | The type in the `implements` clause (or `where` predicate) that names a struct, class, enum or built-in type: K has no inheritance. | — |
| K2015 | `interface-member-mismatch` | error | The implementing member's name token; label: the requirement's name, `required here`. | — |
| K2016 | `stored-field-in-extension` | error | The field's name token. | — |
| K2017 | `unwrap-required` | error | The optional receiver of the `.` member access. | ``Use `?.` ``: replace the `.` with `?.` (maybe-incorrect). |
| K2018 | `needless-optional-operator` | warning | The `?.` or `??` token whose left operand can't be `nil`. | For `?.`, ``Replace `?.` with `.` `` (machine-applicable); for `??`, ``Remove `?? <default>` `` deleting from the whitespace before `??` to the end of the right operand (machine-applicable). |
| K2019 | `duplicate-member` | error | The later member's name; label `first declared here` at the first. Members share a name when both are fields, computed properties or cases in the same namespace: instance members, or static members and cases (`.x`). Functions may be overloaded and may share a name with a property. The first member wins for the memberwise initializer, lookups and derivation. | — |
| K2020 | `type-argument-count` | error | The `<…>` list: of a call's explicit type arguments (`decode<Int, String>(…)`, `count<Int>(…)`) when their number differs from the generic parameters of the function, member or type they are given to, and of a type (`Box<Int, Int>`) likewise. A wrong count binds the parameters to `{error}`, so nothing that depends on them is reported. | Remove the list when the name isn't generic (machine-applicable). |

Messages and detail lines for this range that the corpus pins exactly:

- **K2001** message: ``expected `T`, found `U` ``, both types rendered as in [sema.md §2](sema.md#2-public-api)
  (`String?`, `List<corpus.Item>`); for a `nil` literal where `T` isn't optional, `U` is `nil`
  (``expected `Int`, found `nil` ``).
- **K2011** label at the `let` keyword of the binding or field: ``declared with `let` here``. The fix
  ``Change `let` to `var` `` (maybe-incorrect) replaces that keyword, and is offered only for a local
  binding (not for fields, top-level `let`s or `static let`s).
- **K2012** fix ``Simplify to `R` `` (machine-applicable) replaces the whole comparison with `R`: the
  operand's text for `== true`, `!= false` and `true ==`, or `not ` and the operand's text for `== false`
  and `!= true` (a parenthesized operand keeps its parentheses: `not (a or b)`). A `Bool?` operand is not
  linted, and a non-`Bool` operand is K2013.

### Units and number extensions (K2100–K2199)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K2101 | `mixed-unit-dimensions` | error | The binary expression. | — |

### Effects: async and throws (K2200–K2299)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K2201 | `missing-try` | error | The call expression (including `await` when present). | Insert `try ` before the covering expression: the outermost operand or postfix chain the call starts, since `try` must start what it covers (K0114). Machine-applicable when the error then reaches a handler (an enclosing `do`/`catch`, or a `throws` that names it); otherwise maybe-incorrect, because the edit alone turns K2201 into K2203. |
| K2202 | `needless-try` | warning | The `try` token. | Remove `try ` (machine-applicable). |
| K2203 | `unhandled-error` | error | The `try` expression or `throw` statement; label: the enclosing signature. | Add or widen `throws` (maybe-incorrect). |
| K2204 | `missing-await` | error | The call or query expression. | Insert `await ` before the covering expression, as for K2201 (machine-applicable inside an async function). |
| K2205 | `await-outside-async` | error | The `await` token; label: the enclosing signature. | Add `async` to the function (maybe-incorrect). |
| K2206 | `catch-not-exhaustive` | error | The `do` keyword; a note lists the missing patterns. | Insert `catch` clauses for the missing cases (maybe-incorrect). |

### Control flow (K2300–K2399)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K2301 | `match-not-exhaustive` | error | The `match` keyword; a note lists the missing patterns. | Insert arms for the missing cases (maybe-incorrect). |
| K2302 | `unreachable-arm` | error | The arm's pattern, or the `default`/`catch` keyword. | Remove the arm (maybe-incorrect). |
| K2303 | `wildcard-case-arm` | error | From `case` to `_` inclusive. | Replace with `default` (machine-applicable). |
| K2304 | `unused-catch-all-binding` | error | The binding pattern (`let x`). | Replace the arm head with `default`, or the pattern with `_` (machine-applicable). |
| K2305 | `guard-does-not-exit` | error | The `else` keyword. | — |
| K2306 | `use-before-initialization` | error | The read; label: the declaration. | — |
| K2307 | `missing-return` | error | The closing `}` of the body. | — |
| K2308 | `return-in-lambda` | error | The `return` keyword. | Remove `return` when it is the last statement (machine-applicable). |
| K2309 | `misplaced-jump` | error | The keyword. | — |
| K2310 | `for-over-stream` | error | The `for` keyword. | Insert `await ` after `for` (machine-applicable inside an async function). |
| K2311 | `for-await-not-stream` | error | The `await` token. | Remove `await ` when the operand is a collection (machine-applicable). |
| K2312 | `case-of-unknown-enum` | error | The case pattern (`DeviceError.cancelled`), when the matched value's type isn't an enum (an `Error`, such as a catch-all's binding over two error types, or a type parameter): enum values don't carry their enum, so the case couldn't be told from another enum's at the same position. Help: catch the case in the `do` that throws it, or match a value of the enum's own type. | — |

### Stdlib-specific rules (K2400–K2499)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K2401 | `named-status-as-other` | error | The integer literal. | Replace with the named case (machine-applicable). |
| K2402 | `money-sum-without-currency` | error | From the name `sum` to the end of the call `sum()` on a `List<Money>`. | Replace with `sum(in: .usd) { amount => amount }` (maybe-incorrect: the currency is a placeholder to choose). `sum()` on a list of any element type other than `Int`, `Double` or `Money` is K2009 at the same span. |
| K2403 | `literal-required` | error | The argument of an initializer marked `@Literal` (`Url(…)`, `Uuid(…)`, `EmailAddress(…)`, `TimeOfDay(…)`, `StorageKey(name:)`, `HttpStatus(code:)`) that isn't a plain string literal (an interpolated string isn't one) or, for `HttpStatus(code:)`, an integer literal. | Insert `.parse` after the type name (maybe-incorrect: `parse` returns an optional), when the type has a static `parse(_:)`; none otherwise. |
| K2404 | `invalid-literal` | error | The string or integer literal a checked initializer rejects, with the reason in the message; for a money amount, the whole `1.005.usd` expression. The formats are those of the runtime's parsers (`k_sema::literals`): `EmailAddress.parse` with display names, `Url.parse`, `Uuid.parse`, `TimeOfDay` as `HH:MM`, a status code `100...599`, and a money amount with no more significant decimal places than its currency. | — |
| K2405 | `duplicate-storage-key` | error | The name literal of every `StorageKey(name:)` constant or field, in the app's non-test code, whose name an earlier key (in file path order) already has, with a label at that key's literal. | — |
| K2406 | `conflicting-drag-kind` | error | The name literal of every `DragKind(name:)` constant or field (test code included) whose name an earlier kind (in file path order) has with another payload type, with a label at that kind's literal naming its payload. | — |
| K2407 | `mixed-currencies` | error | The whole `+` or `-` expression whose two `Money` operands have visible, different currencies: a money literal (negated or parenthesized too), a `Money * Int` scaling of one, or a sum or difference of such operands. | — |
| K2408 | `conflicting-channel` | error | The name literal of every `TabChannel(name:)` (or `DataChannelKind(name:)`) constant or field (test code included) whose name an earlier one of the same type (in file path order) has with another message type, with a label at that one's literal naming its message type. | — |
| K2409 | `colliding-data-channels` | error | The name literal of every `DataChannelKind(name:)` constant or field (test code included) whose name differs from an earlier one's (in file path order) but whose channel id is the same (1 plus the 32-bit FNV-1a hash of the name's UTF-8 bytes modulo 1023, which `runtime/rtc.js` derives too; [46-media-and-rtc.md](../design/46-media-and-rtc.md#typed-data-channels)), with a label at the earlier kind's literal naming the id. | — |
| K2410 | `duplicate-collection` | error | The name literal of every `Collection(name:)` constant or field, in the app's non-test code, whose name an earlier collection (in file path order) already has, and the collection literal of every `CollectionIndex(collection:field:)` on a collection and field an earlier index already has, with a label at the earlier one's literal ([52-indexed-collections.md](../design/52-indexed-collections.md#declarations)). | — |
| K2411 | `invalid-collection-field` | error | The `key:` or `field:` argument of a `Collection` or `CollectionIndex` that isn't a string literal, or names no stored field of the value type (which must be a struct), a field whose type isn't the declared key or field type (`Field?` too for an index; never optional for a key), or one whose type isn't a key type (`String`, `Int`, `Uuid`, `Date`, an enum without payloads); and the whole call of a `Collection(…)` or `CollectionIndex(…)` written in non-test code anywhere but the initializer of a constant or field. Not reported in test code. | — |
| K2412 | `unknown-collection` | error | The collection literal of a non-test `CollectionIndex(collection:field:)` naming no declared collection, or one whose value type differs (with a label at that collection's literal). | — |

### Annotations, members and reactivity (K3000–K3099)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K3001 | `annotation-mutability` | error | The `let` or `var` token. | Swap `let` and `var` (machine-applicable; maybe-incorrect when turning a `var` into a `let` that its declaration assigns, since the assignment would then be K3014). |
| K3002 | `annotation-not-allowed` | error | The annotation, from `@` to the end of its name. | Remove the annotation (maybe-incorrect). |
| K3003 | `missing-view` | error | The declaration's name token. | — |
| K3004 | `duplicate-block` | error | The keyword of the second block; label: the first. | — |
| K3005 | `invalid-annotated-type` | error | The field's type. `@Slot` takes `View`, `View?` or a function returning `View`; `@Stylable` takes `Classes`; `@Emits` takes `Event<T>`. | — |
| K3010 | `write-to-derived` | error | The assigned place; label: the `@Derived` field. | — |
| K3011 | `state-write-in-derived` | error | The assignment or mutating call. | — |
| K3012 | `throwing-effect-or-handler` | error | The `throws` clause, from `throws` to the end of its type. | — |
| K3013 | `discarded-timer-handle` | warning | The call expression whose `TimerHandle` or `k.device.Subscription` an `@Effect` discards. | Bind it and cancel it in `onCleanup` (maybe-incorrect). |
| K3014 | `write-to-prop` | error | The assigned place (left side of `=` or a compound assignment, or the receiver of a mutating call); label `declared `@Prop` here` at the field's name. Not reported when the field already has K3001. | Change `@Prop let` to `@Binding var`: two edits, the annotation and the `let` token (maybe-incorrect). |
| K3015 | `not-bindable` | error | The operand of `bind`. Bindable places are `@State`, `@Binding` and `@Form` fields, paths into them, and loop variables over a bindable list (`bind item.quantity`). | — |
| K3016 | `missing-bind` | error | The argument expression passed to a `@Binding` prop or a `Binding<T>` element argument. | Insert `bind ` before it (machine-applicable) when the expression is a bindable place; no fix otherwise. |
| K3017 | `throwing-handler` | error | The handler argument of `.on(…)` that names a `throws` function. | Wrap the function's body in `do`/`catch` (maybe-incorrect). |
| K3018 | `unexpected-bind` | error | From `bind` to the start of its operand (the keyword and the space after it). | Remove `bind ` (machine-applicable). |
| K3020 | `load-cycle` | error | The name token of each `@Load` field in the cycle, one diagnostic per field; labels: the reads. | — |
| K3021 | `load-not-codable` | error | The field's type. | — |
| K3022 | `missing-loading-block` | error | The declaration's name token: one with `@Load` fields, or a `@Rendering(.client)` component (47-client-rendered-components.md). | Insert an empty `loading { }` (maybe-incorrect). |
| K3023 | `reserved-reload` | error | The name token of the member named `reload` in a declaration with `@Load` fields. | — |
| K3024 | `load-error-not-codable` | error | The `@Load` field's initializer expression (the error type is declared elsewhere). | — |
| K3025 | `status-outside-failed` | error | The `status(…)` call expression, in a page-level `metadata` block or directly in `failed`. | — |
| K3026 | `derived-cycle` | error | The name token of each `@Derived` field in the cycle, one diagnostic per field. The graph has an edge for every read of another `@Derived` field of the same declaration (`b` or `self.b`) in an initializer, `untracked { … }` included. | — |
| K3027 | `page-without-title` | warning | The page's name token. Only in a program with an `app`; a page has a title when its page-level `metadata` block, the layout it renders in (its own `@Layout`, else the app's, none with `@NoLayout`) or the app's `metadata` calls `title(…)`. Endpoint pages have no metadata and are skipped. | Insert `title("…")` from the page's name, in its `metadata` block or a new one (maybe-incorrect). |
| K3028 | `long-description` | warning | The string literal of a `description(…)` in any `metadata` block (page, layout, app or inside `failed`) whose plain text is over 160 characters. Interpolated descriptions are not measured. | — |

### Views (K3100–K3199)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K3101 | `unknown-element` | error | The element name token. | Replace an HTML name with its K element (maybe-incorrect). |
| K3102 | `missing-element-argument` | error | As K2003. | Insert the label (maybe-incorrect). |
| K3103 | `unknown-attribute` | error | The name after `.`. | The closest attribute (maybe-incorrect). |
| K3104 | `duplicate-attribute` | error | The second chained name; label: the first. | — |
| K3105 | `unknown-event` | error | The event label. | — |
| K3106 | `missing-for-key` | error | Empty span after the loop's operand, where ` key …` goes. | Insert ` key item.id` when the item type has an `id` (maybe-incorrect). |
| K3107 | `content-not-allowed` | error | The offending child. | — |
| K3108 | `multiple-events-in-on` | error | The label of the second event in the call. | Split into one `.on(…)` per event (machine-applicable). |
| K3109 | `invalid-heading-level` | error | The `level:` argument expression (not an integer literal from 1 to 6). | — |
| K3110 | `redundant-role` | error | The implicit member expression, from `.` to the name; help ``use `nav` `` naming the element. | — |
| K3111 | `key-outside-view` | error | From `key` to the end of the key expression. | Remove the `key` clause, with the space before it (machine-applicable). |
| K3112 | `missing-captions` | warning | The `source:` argument of a `video` whose source is written `.file(…)` or `.url(…)` (an implicit member, called or not) and that chains no `.captions(…)` (WCAG 1.2.2, [46-media-and-rtc.md](../design/46-media-and-rtc.md#showing-and-playing-media-video-and-audio)). A `.stream(…)` source, and a source the compiler can't see the case of, are not reported; help names both `Captions` cases. | Insert `.captions(.notNeeded)` right after the element's argument list (maybe-incorrect: a video with speech needs a `.track`). |
| K3113 | `misplaced-element-ref` | error | The name of an `ElementRef` field that isn't a plain `let` (no `var`, `static` or annotation) of a component, page or layout initialized with `ElementRef()`; an `ElementRef()` call anywhere else in non-test code; or the argument of a `.ref(…)` that isn't such a field of the declaration whose view it is ([53-element-references.md](../design/53-element-references.md#the-checks)). A `.ref(…)` naming a field already reported is not reported again. | — |
| K3114 | `element-ref-reused` | error | The argument of a second `.ref(…)` of the same reference, with a label at the first; or of a `.ref(…)` inside a `for` body or a lambda with parameters (a scoped slot). | — |
| K3115 | `unattached-element-ref` | warning | The name of a valid reference field that no `.ref(…)` of its declaration's views attaches. | — |
| K3116 | `element-ref-during-render` | error | The call of `ElementRef.focus`, `blur` or `scrollIntoView` in code that may run while a view renders: a `view`, `loading` or `failed` block outside handlers, or a field initializer (`@Derived`, `@Load` included); or the call there of one of the declaration's own methods whose body calls one on an `ElementRef` field outside a lambda, with a label at that operation (one level of own methods; [53-element-references.md](../design/53-element-references.md#as-built)). | — |

### Styles (K3200–K3299)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K3201 | `unknown-css-property` | error | The property name token. | The closest property, or the camelCase form of a kebab-case name (machine-applicable for the camelCase form). |
| K3202 | `invalid-css-value` | error | The first value component that fails to match, or the whole value when it is incomplete. | — |
| K3203 | `unknown-class` | error | The class name after `.`. | The closest class (maybe-incorrect). |
| K3204 | `unused-class` | warning | The class selector's name. | — |
| K3205 | `unknown-keyframes` | error | The name after `.`. | The closest keyframes name (maybe-incorrect). |
| K3206 | `unused-keyframes` | warning | The keyframes name after `@keyframes`. | — |
| K3207 | `invalid-keyframe-selector` | error | The selector. | — |
| K3208 | `keyframe-rule-content` | error | The nested rule or at-rule. | — |
| K3209 | `unapplied-stylable` | error | The `@Stylable` field's name. | — |
| K3210 | `non-reactive-interpolation` | error | The expression inside `{ }`. | For a theme token or literal: remove the braces, replacing the interpolation with the expression (machine-applicable). |
| K3211 | `unknown-media-feature` | error | The feature name. | The closest feature (maybe-incorrect). |
| K3212 | `unknown-at-rule` | error | The at-rule's name after `@`. | — |
| K3213 | `duplicate-keyframes` | error | The second keyframes name; label `first declared here` at the first. | — |
| K3214 | `kebab-case-css-keyword` | error | The keyword token. | Its camel-case form (machine-applicable). |

### Themes (K3300–K3399)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K3301 | `unknown-theme-token` | error | The token name. | The closest token (maybe-incorrect). |
| K3302 | `redeclared-theme-token` | error | The token name in the declaration; label: the library's declaration. | Rewrite as an override (machine-applicable). |
| K3303 | `override-outside-theme` | error | The whole member. | — |
| K3304 | `unknown-token-category` | error | The category word. | The closest category (maybe-incorrect). |
| K3305 | `token-category-mismatch` | error | The declared type. | Replace it with the category's type (machine-applicable). |
| K3306 | `variant-named-base` | error | The variant's name. | — |
| K3307 | `duplicate-theme-member` | error | The second token, variant or override name; label `first declared here` at the first. | — |

#### Quick-fix and label wording (K3000–K3399)

The `sema/ui` corpus cases pin these texts exactly (a `.diag` entry with detail lines compares them):

| Code | Text |
|---|---|
| K3001 | fix ``Change `let` to `var` `` / ``Change `var` to `let` ``, one edit replacing the token |
| K3002 | fix `Remove the annotation` (maybe-incorrect), one edit removing the annotation and the spaces after it |
| K3010 | label ``declared `@Derived` here`` at the field's name |
| K3014 | label ``declared `@Prop` here``; fix ``Change `@Prop let` to `@Binding var` `` |
| K3016 | fix ``Insert `bind ` `` (edit: empty span at the argument's start, text `bind `) |
| K3018 | fix ``Remove `bind ` `` |
| K3104 | label `first set here` at the first name |
| K3101 | fix ``Replace `a` with `link` `` (maybe-incorrect); for `h1`–`h6` without an argument list the replacement is `heading(level: n)` |
| K3106 | fix ``Insert ` key item.id` `` (maybe-incorrect), naming the loop variable |
| K3111 | fix ``Remove the `key` clause`` |
| K3112 | help naming `.captions(.track(url:language:label:))` and `.captions(.notNeeded)`; fix ``Insert `.captions(.notNeeded)` `` (maybe-incorrect) |
| K3113, K3115, K3116 | help saying where the reference is declared, attached or called instead |
| K3114 | label `attached here` at the first `.ref(…)`'s argument; help naming the fix (a reference per element, or a component per row) |
| K3201 | fix ``Replace `font-size` with `fontSize` `` (machine-applicable for the camel-case form, maybe-incorrect for the closest name) |
| K3214 | fix ``Replace `space-between` with `spaceBetween` `` (machine-applicable) |
| K3210 | fix `Remove the braces` |
| K3302 | label `declared here` at the library's token; fix `Rewrite as an override`, one edit replacing the category, name and type with the name and a space |
| K3110 | help ``use `nav` `` |
| K3213, K3307 | label `first declared here` |
| K2004, K3103, K3203, K3205, K3211, K3301, K3304 (closest name) | fix ``Replace `old` with `new` `` (maybe-incorrect); K1101, K1203 and K2006 say ``Replace with `new` `` |
| K3305 | fix ``Replace `Length` with `Spacing` `` (machine-applicable) |

### Services, DI, layering and the server/client boundary (K4000–K4999)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K4001 | `missing-implementation` | error | The type of the `@Inject` field. | — |
| K4002 | `ambiguous-implementation` | error | The type of the `@Inject` field; labels: the implementations. | Insert `use X for I` in `App.k` (maybe-incorrect). |
| K4003 | `dependency-cycle` | error | The type of the `@Inject` field that closes the cycle in each participant; labels: the path. | — |
| K4004 | `scope-violation` | error | The type of the `@Inject` field; a note names the injection path. | — |
| K4005 | `layering-violation` | error | The type of the `@Inject` field. | — |
| K4006 | `foreign-state-write` | error | The assigned place; label: the field. | — |
| K4007 | `server-only-in-client` | error | The use (an import path, an `@Inject` type or a name); a note names the path to the client. Also the member name of a `PublicUrls.absolute(endpoint:)` call in code that may run in the browser (a shared service, or client code); a note says endpoint URLs are server-only, and, for a shared service, one says to mark it `@ServerOnly` ([12-routing.md](../design/12-routing.md#absolute-urls-publicurls)). | — |
| K4008 | `service-on-both-sides` | error | The service's name token; notes name both paths. | — |
| K4009 | `job-reaches-session` | error | The job's name token; a note names the path. | — |
| K4010 | `use-target-mismatch` | error | The operand before `for`. | — |
| K4011 | `use-expression-not-allowed` | error | The operand expression. | — |
| K4012 | `client-only-during-ssr` | error | The call, or the member expression reading a member of a client-only platform interface other than `isSupported` and `permission` (45-web-platform.md, "Server rendering and the server"). Not reported inside a client-rendered component's subtree (47-client-rendered-components.md). | — |
| K4013 | `client-state-in-server-render` | warning | The read. Not reported inside a client-rendered component's subtree (47-client-rendered-components.md). | In a component, first: add `@Rendering(.client)` to the component, with an empty `loading { }` when it has none (maybe-incorrect); then add `@Rendering(.client)` to the app (maybe-incorrect). |
| K4014 | `use-restates-default` | warning | The whole `use` rule, from `use` to the end of its target type. | Remove the rule and its line, and the imports only the rule used, so no K1105 follows (machine-applicable). |
| K4015 | `environment-not-covered` | error | The target type (after `for`) of the interface's first `use` rule in `App.k`; a note names each uncovered environment. | — |
| K4016 | `invalid-service-init` | error | The parameter list of an `init` with parameters; the throws clause (from `throws` to the end of its type) of a throwing one; the `init` keyword of a second `init`. | — |
| K4017 | `config-member-not-allowed` | error | The name token of a config field without `@Env`, or of a function in a config. | — |
| K4018 | `duplicate-env-variable` | error | The later `@Env` argument's string literal (by path, then offset), quotes included; label: the earlier one. | — |
| K4019 | `not-a-config-value` | error | The config field's type. | — |
| K4020 | `config-construction` | error | The call expression that constructs the config, outside test code (`App.k`'s `use` operands are K4011). | — |
| K4021 | `implements-service` | error | The service's name in the `implements` clause, outside test files and test-only modules. | — |
| K4022 | `endpoint-session-not-injected` | error | The endpoint page's name token; a note names the path to `Session`/`SessionManager`. | Insert `@Inject let session: Session` (maybe-incorrect). |
| K4023 | `missing-remote-rule` | error | The type of the `@Inject` field of a protocol from a library dependency that the app neither implements nor binds with `use remote(…) for P`; a note shows the rule to add. Apps only; test code is exempt. | — |
| K4024 | `remote-rule-not-allowed` | error | The target type of a `remote` rule that isn't a protocol, is implemented by the app (the message names the implementation) or is declared in the app itself (a note says to move it into a library). | — |
| K4025 | `not-a-remote-service` | error | The argument of `remote(…)` that isn't a config implementing `RemoteService` (or the whole call when it isn't one name); for a `RemoteService` config, its name when `url: Url` or `token: Secret<String>` is missing, or the type of a `url`, `token` or `timeout` field of the wrong type. | — |
| K4026 | `remote-client-not-a-protocol` | error | `at(…)` of a `RemoteClient.at` call whose result type resolves to a declaration that isn't a protocol. | — |
| K4027 | `server-rendering-on-component` | error | The `@Rendering(.server)` annotation on a component. | Remove the annotation (maybe-incorrect). |
| K4028 | `loading-reads-component-state` | error | The read (a name, `self.name` or a call) of a member that isn't a `@Prop` or static, in a `@Rendering(.client)` component's `loading`; a label at the member. | — |
| K4029 | `redundant-client-rendering` | warning | `@Rendering(.client)` on a component of a `@Rendering(.client)` app. | Remove the annotation (machine-applicable). |
| K4030 | `user-action-required` | error | The call of a `@UserAction` member, or of a function inferred user-action-only (it calls one before its first `await`), in a context that isn't a user's action (an `@Effect`, `init()`, a field, `@Load` or `@Derived` initializer, a view, a timer callback, a lambda passed as a value, a handler of an event that isn't activation-triggering), with help; or such a call after an `await` in a function or user's action, with a label at the `await`; or a second call of a consuming member (`@UserAction(consuming: true)`) on one path, with a label at the first; or a user-action-only function used as a value anywhere but directly in `.on(…)` of an activation-triggering event or `form.handle(…)`: the reference; or a user-action-only function implementing an interface member not declared `@UserAction`: its name, with a label at the requirement. A note gives the path to the `@UserAction` member when it goes through other functions. Test suites aren't checked. | On the last form, declare the requirement `@UserAction` (maybe-incorrect). |
| K4031 | `handshake-header-on-client` | error | The name of a `headers`, `secretHeaders` or `bearerToken` setter of `k.http.WebSocketRequest` chained in a declaration that may run in the browser (shared or client-only, as `di::side` computes it), since a browser can't send headers on a WebSocket handshake ([44-channels.md](../design/44-channels.md#raw-websockets-khttp)). | — |
| K4032 | `bare-private-key-in-config` | error | The type of a config field that holds a secret or private key of `k.crypto` (`AesGcmKey`, `Ed25519PrivateKey`, `EcdsaP256PrivateKey`, `X25519PrivateKey`, bare, optional or as a list's element) without `Secret<…>`, which it needs so the key may come from `NAME_FILE` and is never printed ([33-crypto.md](../design/33-crypto.md#keys-in-configuration)); help names the forwarded operation. Public key types are plain `ConfigValue`s. | Wrap the type in `Secret<…>`, keeping a trailing `?` outside (machine-applicable). |
| K4033 | `client-only-on-server` | error | Each reference, in a server-only declaration (a protocol implementation, a `@Scope(.server)` or `.request` service, a job, an entity, a repository, a config, an endpoint page: `di::side`), to a stdlib type marked `@ClientOnly` that is a class, struct, enum or component (`MediaStream`, `MediaTrack`, `MediaController`, `AudioMeter`, `RecordingSession`, `PeerConnection`, `MediaSender`, `DataChannel`; [46-media-and-rtc.md](../design/46-media-and-rtc.md#server-and-client)). A client-only service or interface named there is K4008's (through its injection) or K4007's. | — |
| K4034 | `app-updates-without-offline` | warning | The type of each `@Inject` field naming `k.device.AppUpdates` outside a test group, in a project whose `app` isn't `@Offline` (`di::check`; [54-offline.md](../design/54-offline.md#updates)): no service worker caches the build, so `isAvailable` is always `false`. | "Add `@Offline` to the app" (inserts `@Offline` before the app's first token) |
| K4035 | `worker-member-not-allowed` | error | Each member of a `@Worker` service that would keep state or face the page (`workers::check`; [55-workers.md](../design/55-workers.md#declaring-a-worker)): a `@State`, `@Derived` or `var` field, a `let` without an initializer, a field or computed property that isn't `private`, `init()`, an `@Effect` function, an `rpc`, a `static` function that isn't `private`, and `@Scope(…)` on the service. | "Make it `private`" (a constant, a computed property, a static function); "Remove `init()`"; "Remove `@Scope`" |
| K4036 | `worker-injection` | error | Each `@Inject` field of a `@Worker` service, at its name: a worker has no container. | "Remove the injected field" |
| K4037 | `worker-method-signature` | error | A method of a `@Worker` service that isn't `private` and isn't `async`, declares no `throws` (at its name), or is generic (at its generic parameters). | "Add `async`", "Add `throws WorkerOnlyError`" or "Add `async throws WorkerOnlyError`", importing `k.concurrency.WorkerOnlyError` |
| K4038 | `worker-error-without-failure-case` | error | A worker method's `throws` type that is neither `WorkerOnlyError` nor an enum with exactly one `workerFailure(WorkerFailure)` case, with a label at the type. | "Add `case workerFailure(WorkerFailure)`" (a project enum without the case), importing `k.concurrency.WorkerFailure` |
| K4039 | `worker-payload-not-codable` | error | A worker method's parameter, result or error type that isn't `Codable`, at the type; a `Stream<T>` result gets a note that streamed progress isn't supported yet. | — |

### Pages and routing (K5000–K5499)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K5001 | `url-conflict` | error | The later page's name token or `at` template; label: the first. Also a page or endpoint page at `/_k/channels`, where the browser opens its channels' WebSocket ([44-channels.md](../design/44-channels.md#sessions-and-security)): its URL, with no label. | — |
| K5002 | `unroutable-param` | error | The field's type. | — |
| K5003 | `at-template-mismatch` | error | The template segment, or the field's name token when the template lacks it. | — |
| K5004 | `endpoint-without-at` | error | The page's name token. | Insert ` at "/<kebab-name>"` (machine-applicable). |
| K5005 | `guard-on-endpoint` | error | The guard expression in the `requires` clause. | — |
| K5006 | `not-a-guard` | error | The expression. | — |
| K5007 | `missing-sign-in-page` | error | The app's name token in `App.k` (only app projects report it). | — |
| K5008 | `sign-in-page-requires-authenticated` | error | The `Authenticated` guard in the sign-in page's `requires` clause. | — |
| K5009 | `layout-on-endpoint` | error | The annotation, from `@` to the end of its name. | — |
| K5010 | `endpoint-member-not-allowed` | error | The keyword of the `view`, `metadata`, `loading`, `failed` or `style` block. | — |
| K5011 | `duplicate-handler-method` | error | The later handler's method annotation, from `@` to the end of its name; label: the first. | — |
| K5012 | `invalid-handler-signature` | error | The handler function's name token. | — |
| K5013 | `set-cookie-header` | error | The string literal key `set-cookie` (in any case) of a map literal passed to `HttpResponse.headers(…)`. A computed name panics at run time instead. | — |
| K5014 | `guard-on-layout` | error | The `requires` clause of a layout, whatever its guards name (they aren't resolved). The server checks the guards of the page it answers, never its layout's. | — (a help names the layout: put the clause on each page that renders in it) |

### Protocols and RPC (K5500–K5999)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K5501 | `rpc-without-throws` | error | The rpc's name token. | Insert ` throws RpcOnlyError` before the return type (machine-applicable). |
| K5502 | `rpc-error-without-failure-case` | error | The error type in the rpc's `throws` clause, or a channel rpc's refusal `E` in its `Channel<C, S, E>` result; label: the enum. | Insert `case rpcFailure(RpcFailure)` into the enum (machine-applicable when the enum is in the project). |
| K5503 | `rpc-payload-not-codable` | error | The parameter's, return or error type; for a channel rpc, the `C`, `S` or `E` type argument of its `Channel<C, S, E>` result. | — |
| K5504 | `rpc-implementation-not-server` | error | The protocol's name in the `implements` clause. | — |
| K5505 | `rpc-handler-mismatch` | error | The handler's name token; label: the protocol's rpc. | — |
| K5506 | `handler-throws-rpc-failure` | error | The `throw` statement, in the body of a protocol handler, whose value constructs a `rpcFailure` case ([28-rpc-wire-format.md](../design/28-rpc-wire-format.md): only the runtime produces it). Client code rethrowing it is fine. | — |
| K5507 | `requires-on-handler` | error | The `requires` clause of an rpc outside a protocol: a service's handler, or an rpc no protocol declares. Only the protocol's clauses (on the protocol and on its rpc) are checked by the server. A label marks each protocol rpc the handler serves. | Move the guards the protocol doesn't already declare to every protocol rpc the handler serves (appended to its `requires` clause, or a new clause after its signature), importing them where needed, and delete the handler's clause (maybe-incorrect: it changes the protocol's contract for every implementation and caller). When the protocol already declares every guard: delete the clause (machine-applicable). Either way, also delete the imports of the handler's file that only the clause used, except those of guards moved to a protocol in the same file. No fix when a protocol comes from a library or the stdlib (a help says so), or when no protocol declares the rpc (a help says to declare it in one). |
| K5508 | `rpc-outside-protocol` | error | The `rpc` keyword of a member of anything but a protocol (a struct, a component, an extension, or a service) that no protocol in its declaration's `implements` clause declares. A struct that names such a protocol is K5504 instead. | Replace `rpc` with `func` (maybe-incorrect: the rpc may belong in a protocol). None when the rpc has a `requires` clause, or no body outside an interface. |
| K5509 | `channel-result-misplaced` | error | The return type of a `func` or rpc whose result is `Channel<…>` or `Channel<…>?` but that isn't a protocol's rpc or a service's handler serving one; or the `Channel<…>?` result of a protocol rpc or handler ([44-channels.md](../design/44-channels.md#channelc-s-e)). | — |
| K5510 | `nested-channel` | error | A type argument that names `Channel` (through `?` and parentheses), anywhere a type is written: `List<Channel<…>>`, `Map<K, Channel<…>?>`, `Stream<Channel<…>>`. A `Channel<…>?` that isn't a type argument is allowed. | — |
| K5511 | `channel-on-server` | error | In the body of a server-only declaration (a service implementing a protocol, a `.request` or `.server` service, an endpoint page, …): the member expression or call of a `Channel`'s `state`, `messages`, `send` or `close`, or the call of a channel rpc ([44-channels.md](../design/44-channels.md#server-only-and-client-only)). | — |
| K5512 | `channel-built-outside-handler` | error | The type name of a `Channel(sending:receiving:)` call outside the body of a service's handler of a channel rpc (its lambdas included); a private helper of the service is outside. | — |
| K5513 | `channel-opened-during-ssr` | warning | The call in a `@Load` initializer of a channel rpc, or of an app function that calls one, directly or through other app functions (resolved names only), except in a client-rendered app or a client-rendered component's subtree. | — |
| K5514 | `generic-rpc` | error | The generic parameter list of a protocol (its rpcs are then not checked further) or of an rpc, in a protocol or a service; a service rpc that is generic, or serves a generic one, isn't compared with the protocol's (no K5505). | — |

### Persistence (K6000–K6999)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K6001 | `untranslatable-query` | error | The smallest untranslatable expression. | — |
| K6002 | `unsupported-column-type` | error | The field's type. | — |
| K6003 | `reserved-table-prefix` | error | The entity's name token. | — |
| K6004 | `invalid-migration` | error | The builder call that doesn't replay (the step inside `alterTable` when it is one), or the statement that isn't a builder call; reported by `k_codegen::project_diagnostics` and the build. | — |
| K6005 | `destructive-migration` | error | The destructive call (`dropColumn` of a retired column, `dropTable` of a retired table, `renameEnumCase`, `removeEnumCase`) in a migration without `@Breaking`; help: add it. | — |
| K6006 | `live-schema-change` | error | The `dropColumn`, `dropTable`, `renameColumn` or `renameTable` of something no earlier migration retired, without `@Breaking("single instance")`; a note when another reason is given; help: the two-release sequence. | — |

### Forms and validation (K7000–K7499)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K7001 | `form-value-outside-handle` | error | The `value` member name. | — |
| K7002 | `empty-outside-form` | error | The implicit member expression. | — |
| K7003 | `format-on-string` | error | The annotation, from `@` to the end of its name. | — |
| K7004 | `redundant-email` | warning | The annotation, from `@` to the end of its name. | Remove it, or rewrite `@Email(message:)` as `@Format(message:)` (machine-applicable). |
| K7005 | `matches-not-literal` | error | The argument. | — |
| K7006 | `invalid-regex` | error | The string literal, or the offending part of it when the regex parser reports a position. | — |

### Tests (K7500–K7999)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K7501 | `expect-not-bool` | error | The condition. | — |
| K7502 | `expect-throws-cannot-throw` | error | The `expect` keyword. | — |
| K7503 | `unbound-test-config` | error | The test group's name token; a note names the config and the injection path. | — |
| K7510 | `test-failed` | error | Reported by `k test`, and by `k check`, which runs the tests of a program without errors (`--no-tests` skips them): the condition of the failed `expect`, the pattern of the failed `expect throws`, or the test's name when it panicked, threw, never finished or a query action failed; notes give each operand's value and the test's full name. | — |

### Lockfiles, manifest and packages (K8000–K8999)

| Code | Name | Severity | Primary span | Fix |
|---|---|---|---|---|
| K8001 | `breaking-change` | error | The declaration or member that changed; a note names the lockfile entry. For `rpc.lock`: the protocol's name (the app's name for a protocol no longer served), one note per change, a label at the protocol's line in the lock. | — |
| K8002 | `invalid-lockfile` | error | The offending line, or offset 0. | — |
| K8003 | `incompatible-job-payload` | error | The payload type argument of the job's `implements Job<…>`; one note per incompatible change, then a note on how to proceed. | — |
| K8004 | `schema-drift` | error | The field (its name token) or entity (its name token) that differs from `schema.lock`, or the lock's `[tables.<name>]` line for a removed entity; a note says why `k migrate new` doesn't write it; help: `k migrate new`, or the `k migrate rename` of a detected rename. | — |
| K8005 | `schema-lock-mismatch` | error | The lock's `[tables.<name>]` line (the first differing column's line when there is one), one note per difference; the first migration's name when there is no lock; help: `k build`. | — |
| K8006 | `migration-order` | error | The migration's name token (label: its entry in the lock), or the lock's entry for a listed migration without its file. | — |
| K8007 | `removed-url` | error | The lock's `[pages.<case>]` or `[endpoints.<case>]` line, or its redirect line; the page's name token (label: the lock's entry) when the page still exists with another URL or was renamed. | — |
| K8008 | `incompatible-storage-key` | error | The key's constant or field name token; one note per incompatible change (a renamed key is one), then a note on how to proceed. | — |
| K8009 | `incompatible-collection` | error | The collection's constant or field name token, matched with `storage.lock`'s `[collections.<name>]` by name, else by `declaration`; one note per incompatible change (a changed key field, which `@Breaking` doesn't allow, comes first; a renamed collection is one), then a note on how to proceed ([52-indexed-collections.md](../design/52-indexed-collections.md#compatibility-k8009)). | — |
| K8010 | `incompatible-key-name` | error | The `KeyName` constant's or field's name token, matched with `storage.lock`'s `[keyNames.<name>]` by name, else by `declaration`; one note per incompatible change (a changed key type, which `@Breaking` doesn't allow, comes first; a renamed key name is one), then a note on how to proceed ([33-crypto.md](../design/33-crypto.md#decided-while-implementing)). | — |
| K8101 | `invalid-manifest` | error | The offending key or line, or offset 0: unreadable TOML, `[package]` without `name` or `k`, a dependency that is neither a git tag nor `{ workspace = true }`, a bad or duplicate `members` entry, a nested workspace, a `[package]` at a workspace root, a package name used twice. | — |
| K8102 | `unknown-dependency` | error | The `[dependencies]` line of `{ workspace = true }` naming no member (a note lists the members), used outside a workspace, or of a git tag (not resolved yet). | — |
| K8103 | `missing-workspace-member` | error | The string in the root's `members` naming a directory without `k.toml`; a note says what to create. | — |
| K8104 | `package-cycle` | error | Each `[dependencies]` line on a cycle between members, in the depending member's `k.toml`; notes: the cycle (`a → b → a`) and the fix. | — |
| K8105 | `app-dependency` | error | The `[dependencies]` line naming an app member; a note says to share a library instead. | — |
| K8106 | `duplicate-module` | error | The `[dependencies]` line that brings in a library declaring a module the member (or an earlier library) declares too. | — |

## Driver codes

| Code | Name | Primary span | Fix |
|---|---|---|---|
| K9001 | `invalid-utf8` | Empty span at the byte offset of the first invalid byte. | — |
| K9002 | `io-error` | Empty span at offset 0 of a placeholder file named after the path. | — |
| K9003 | `tests-not-run` | Empty span at offset 0 of the project's `k.toml`: `k check` couldn't build the test bundle or run it; the message gives the reason. | — |

## Adding a code

1. Add the constant to the `codes!` table in `crates/k_diagnostics/src/codes.rs`, in numeric order, with a
   kebab-case name, default severity and a one-sentence summary.
2. Add a row here with its primary span convention and fixes.
3. Add corpus cases (`.diag`) that produce it, including a case for each fix.
