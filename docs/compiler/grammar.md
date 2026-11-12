# K Grammar

This document is the precise grammar of K: its lexical structure, lexer modes, statement termination, the
syntax of every construct, operator precedence, and the S-expression format used to dump syntax trees in
golden tests. It is normative for `crates/k_syntax`. Where the design documents in [`docs/design`](../design)
leave a syntactic question open, the answer is recorded under [Resolved ambiguities](#18-resolved-ambiguities).

Names in `monospace` like `Ident` or `ComponentDecl` are `SyntaxKind` variants (see
`crates/k_syntax/src/kind.rs`). They are written verbatim in `.tokens` and `.ast` golden files.

Contents:

1. [Notation](#1-notation)
2. [Source text](#2-source-text)
3. [Lexical structure](#3-lexical-structure)
4. [Lexer modes](#4-lexer-modes)
5. [Newlines and termination](#5-newlines-and-termination)
6. [Contextual keywords](#6-contextual-keywords)
7. [Files and imports](#7-files-and-imports)
8. [Declarations](#8-declarations)
9. [Members](#9-members)
10. [Clauses, parameters, arguments and annotations](#10-clauses-parameters-arguments-and-annotations)
11. [Types](#11-types)
12. [Statements](#12-statements)
13. [Expressions](#13-expressions)
14. [Patterns](#14-patterns)
15. [Views, loading, failed and metadata blocks](#15-views-loading-failed-and-metadata-blocks)
16. [Style blocks](#16-style-blocks)
17. [Tests](#17-tests)
18. [Resolved ambiguities](#18-resolved-ambiguities)
19. [Syntax tree dump format](#19-syntax-tree-dump-format)
20. [Token dump format](#20-token-dump-format)

---

## 1. Notation

Productions use EBNF:

| Notation | Meaning |
|---|---|
| `A ::= ...` | Definition |
| `'text'` | A token with fixed text (keyword or punctuation) |
| `IDENT`, `INT_LIT`, ... | A token class (see [§3](#3-lexical-structure)) |
| `X?` `X*` `X+` | Optional, zero or more, one or more |
| `( A \| B )` | Grouping and alternation |
| `(* ... *)` | Comment |

**Production names are node kinds.** A production whose name is in `PascalCase` (`ComponentDecl`, `Block`)
builds a node of exactly that `SyntaxKind`, containing every token and node its right-hand side matched,
in source order. A production whose name is in `lower_case` (`item`, `stmt`) is a helper: it builds no
node, and what it matches becomes part of the enclosing node.

Two helpers are used throughout:

```ebnf
lines(X)  ::= X*        (* elements separated by significant newlines; see §5 *)
name      ::= IDENT | keyword | contextual_keyword   (* any word, including keywords *)
```

`lines(X)` means: zero or more `X`, each element on its own line. Leading, trailing and repeated blank
lines are allowed. Two elements on the same line are an error (K0102).

`name` appears where any word is acceptable because the position makes the meaning unambiguous: member
names after `.`, argument and parameter labels, implicit members (`.class`, `.default`). The token keeps
its keyword kind in the tree (for example `ClassKw` in `.class(.muted)`).

---

## 2. Source text

- Source files are UTF-8. A file that is not valid UTF-8 is rejected before lexing (K9001).
- A byte-order mark (U+FEFF) at offset 0 is part of the first `Whitespace` token. Anywhere else it is an
  unexpected character (K0001).
- Line terminators are `\n`, `\r\n` and a lone `\r`. Each produces one `Newline` token.
- Lines and columns in diagnostics are 1-based; columns count Unicode scalar values.

---

## 3. Lexical structure

The lexer produces a flat, lossless sequence of tokens: the texts of all tokens, trivia included,
concatenate to the source. It never fails; problems are diagnostics. This section describes the default
**code mode**; [§4](#4-lexer-modes) describes strings, interpolations and style blocks.

### 3.1 Trivia

| Kind | Text |
|---|---|
| `Whitespace` | A maximal run of spaces (U+0020) and tabs (U+0009), plus a leading BOM |
| `Newline` | One of `\r\n`, `\n`, `\r` |
| `LineComment` | `//` up to the line terminator (not included), unless it is a doc comment |
| `DocComment` | `///` followed by anything other than `/`, up to the line terminator. `////...` is a `LineComment` |
| `BlockComment` | `/*` ... `*/`. Block comments **nest**: `/* a /* b */ c */` is one token |

An unterminated block comment extends to the end of the file and reports K0003 at the opening `/*`.

Other whitespace characters (form feed, vertical tab, non-breaking space, ...) are unexpected characters.

### 3.2 Unexpected characters

Any character that cannot start a token in the current mode (for example `$`, `` ` ``, `^`, `\` outside a
string, a NUL, or a non-ASCII character outside strings and comments) is an error. A maximal run of such
characters forms **one** `Error` token and **one** K0001 diagnostic whose span is the whole run.

### 3.3 Identifiers and keywords

```ebnf
IDENT ::= [A-Za-z_] [A-Za-z0-9_]*        (* but not a lone "_" and not a hard keyword *)
```

Identifiers are ASCII-only (see [RA-21](#ra-21-ascii-identifiers)). A lone `_` is the `Underscore` token.
`_name` and `__` are identifiers.

**Hard keywords** are never identifiers. Each has its own kind, named after the word with a `Kw` suffix
(`func` is `FuncKw`, `self` is `SelfKw`):

```text
and  as  async  await  bind  break  case  catch  class  continue  default  do  else  enum  expect
extend  false  for  func  guard  if  implements  import  in  init  interface  is  let  match  nil
not  or  private  public  requires  return  rpc  self  static  struct  style  throw  throws  true
try  use  var  when  where  while
```

`as` and `is` are **reserved**: they have no meaning yet and using them as a name is K0108.

**Contextual keywords** are lexed as `Ident`. The parser treats them as keywords only in specific positions
(see [§6](#6-contextual-keywords)) and then *remaps* the token to its keyword kind in the tree:

```text
app  at  component  config  entity  failed  key  layout  loading  matches  metadata  migration
page  proto  protocol  repository  service  test  theme  variant  view
```

### 3.4 Number literals

```ebnf
INT_LIT   ::= dec | '0x' [0-9A-Fa-f_]+ | '0b' [01_]+ | '0o' [0-7_]+
FLOAT_LIT ::= dec '.' digits exponent? | dec exponent
UNIT_LIT  ::= ( dec | dec '.' digits exponent? | dec exponent ) ( unit_suffix | '%' )
dec       ::= [0-9] [0-9_]*
digits    ::= [0-9] [0-9_]*
exponent  ::= [eE] [+-]? digits
unit_suffix ::= [A-Za-z_] [A-Za-z0-9_]*
```

Rules, applied in order:

1. After `0x`, `0b` or `0o`, the lexer consumes `[0-9A-Za-z_]*`. The token is an `IntLit`; if there are no
   digits, a digit is invalid for the base, or the literal ends with `_`, K0006 is reported on the token.
2. A decimal literal ending in `_` (`1_`) reports K0006.
3. `.` continues a number only when it is **immediately followed by a digit**. So `1.5` is one
   `FloatLit`, while `10.minutes` is `IntLit Dot Ident`, `0.2.seconds` is `FloatLit Dot Ident`, `1..<10` is
   `IntLit DotDotLt IntLit`, and `9.99.usd` is `FloatLit Dot Ident`.
4. `e`/`E` starts an exponent only when followed by a digit, or by `+`/`-` and then a digit. `1e9` and
   `2.5e-3` are `FloatLit`s; `1em` is a `UnitLit` with suffix `em`.
5. A number immediately followed by `%` is a `UnitLit` including the `%` (`50%`).
6. A number immediately followed by a letter or `_` is a `UnitLit` whose suffix is the following
   identifier characters (`16px`, `200ms`, `1fr`, `3x`). The number may have an exponent, which rule 4
   decides first: `1e5px`, `2.5e-3px` and `1e5%` are `UnitLit`s. **The lexer does not validate the suffix**; the
   parser reports unknown units (K0112). Known units:

   | Type | Units |
   |---|---|
   | `Length` | `px` `em` `rem` `ch` `vw` `vh` `vmin` `vmax` `dvh` `svh` `lvh` `fr` |
   | `Percent` | `%` |
   | `Angle` | `deg` |
   | `Duration` | `ms` `s` |

7. A leading `-` is never part of a number in code mode: `-4px` is `Minus UnitLit` (unary minus). Style
   mode differs ([§4.4](#44-style-mode)).
8. `.5` is `Dot IntLit` (not a number); `5.` is `IntLit Dot`.

Note that `%` directly after a number is part of the literal, so a remainder of a literal needs spaces:
`10 % 3` (see [RA-4](#ra-4-percent-literal-versus-remainder)).

### 3.5 Color literals

```ebnf
COLOR_LIT ::= '#' [A-Za-z0-9_]*
```

The token is valid when the characters after `#` are exactly 3, 4, 6 or 8 hexadecimal digits
(`#fff`, `#3B82F6`, `#3B82F680`). Otherwise the token is still a `ColorLit` and K0005 is reported on the
whole token (including a lone `#`).

### 3.6 Strings

A string literal is tokenized as `StringStart`, then any number of `StringText`, `StringEscape` and
interpolations, then `StringEnd`:

```text
"Pay {total}!"   →   StringStart "\""  StringText "Pay "  InterpStart "{"  Ident "total"
                     InterpEnd "}"  StringText "!"  StringEnd "\""
```

| Piece | Text |
|---|---|
| `StringText` | A maximal run of characters other than `"`, `\`, `{`, `\r` and `\n`. `}` is ordinary text. |
| `StringEscape` | `\n` `\r` `\t` `\0` `\\` `\"`, `\u{` 1–6 hex digits `}`, or `{{` (a literal `{`) |
| `InterpStart` | A `{` not followed by another `{`. Starts an interpolation ([§4.2](#42-interpolation-mode)) |

Errors inside strings:

| Input | Tokens | Diagnostic |
|---|---|---|
| `\{` | one `StringEscape` `\{` | K0007, with a machine-applicable fix replacing it with `{{` |
| `\u` not followed by `{`, or `\u{...}` whose content is not 1–6 hex digits naming a Unicode scalar value | one `StringEscape` covering `\u` and, if present, `{` up to and including the next `}` on the same line before any `"` | K0008 on the escape |
| `\` followed by any other character | one `StringEscape` covering `\` and that character | K0004 on the escape |
| `\` at the end of the line or file | one `StringEscape` `\` | K0004 on the `\` |
| A line terminator or end of file before the closing `"` | no `StringEnd`; the string ends there | K0002 on the opening `"` |

Strings are single-line ([RA-11](#ra-11-strings-are-single-line)). `}}` is two literal `}` characters.

### 3.7 Punctuation and operators

The lexer uses maximal munch over this table (comments are recognized before `/`):

| Kind | Text | | Kind | Text | | Kind | Text |
|---|---|---|---|---|---|---|---|
| `LParen` | `(` | | `Question` | `?` | | `Plus` | `+` |
| `RParen` | `)` | | `QuestionDot` | `?.` | | `Minus` | `-` |
| `LBrack` | `[` | | `QuestionQuestion` | `??` | | `Star` | `*` |
| `RBrack` | `]` | | `At` | `@` | | `Slash` | `/` |
| `LBrace` | `{` | | `Arrow` | `->` | | `Percent` | `%` |
| `RBrace` | `}` | | `FatArrow` | `=>` | | `PlusEq` | `+=` |
| `Comma` | `,` | | `Eq` | `=` | | `MinusEq` | `-=` |
| `Colon` | `:` | | `EqEq` | `==` | | `StarEq` | `*=` |
| `Semicolon` | `;` | | `BangEq` | `!=` | | `SlashEq` | `/=` |
| `Dot` | `.` | | `Bang` | `!` | | `PercentEq` | `%=` |
| `DotDot` | `..` | | `Lt` | `<` | | `Amp` | `&` |
| `DotDotLt` | `..<` | | `LtEq` | `<=` | | `AmpAmp` | `&&` |
| `DotDotDot` | `...` | | `Gt` | `>` | | `Pipe` | `\|` |
| `Underscore` | `_` | | `GtEq` | `>=` | | `PipePipe` | `\|\|` |
| | | | | | | `Tilde` | `~` |

`Bang`, `AmpAmp`, `PipePipe`, `Pipe`, `DotDot`, `Semicolon` (in code), `Amp` and `Tilde` (in code) are
valid *tokens* but not valid *syntax*: the parser reports them with structured fixes (K0106, K0104, K0105,
K0119, K0103, ...). Lexing them as tokens instead of errors is what makes those fixes possible.

There is no `>>` token: `Map<K, List<V>>` lexes as two `Gt` tokens. The lexer also never joins `>` and `=`
across a generic list; write `List<Int> = []` with a space ([RA-30](#ra-30-generic-close-before-)).

---

## 4. Lexer modes

The lexer keeps a stack of **frames**. The bottom frame is code mode for the whole file. Modes change
only on the rules below, so the lexer stays independent of the parser and token streams are
deterministic, testable in isolation and cheap to relex incrementally.

### 4.1 String mode

Entered on `"` in code, interpolation or style mode. Produces the pieces of [§3.6](#36-strings). Leaves on
the closing `"`, or at a line terminator or end of file (K0002). On `InterpStart` it pushes an
interpolation frame.

### 4.2 Interpolation mode

Lexes code-mode tokens and counts braces: `{` (`LBrace`) increments the depth, `}` at depth 0 is the
`InterpEnd` that pops the frame, any other `}` is `RBrace` and decrements the depth. Nested strings,
lambdas and interpolations are allowed: `"{if a { "x" } else { "y" }}"`.

- **Inside a string**, a line terminator inside the interpolation ends both the interpolation and the
  string: the frames are popped without `InterpEnd`/`StringEnd`, K0002 is reported on the string's
  opening `"`, and the line terminator is lexed as a `Newline` in the enclosing mode. Every frame
  above the string's frame ends too, including a style frame opened inside the interpolation
  (`"{style {` followed by a line break) and a style-value interpolation inside it: strings are
  single-line, so nothing opened within one survives its line. Precisely, a line terminator met
  while any string frame is open pops the lowest string frame and everything above it.
- **Inside a style value**, interpolations may span lines.

An empty interpolation `{}` lexes as `InterpStart InterpEnd`; the parser reports K0128.

### 4.3 Entering style mode

`style` is a hard keyword. When the lexer produces `StyleKw` and the previous non-trivia token is neither
`Dot` nor `QuestionDot`, it looks ahead past whitespace, newlines and comments. If the next character is
`{`, then the trivia is lexed normally, the `{` becomes `LBrace`, and a style frame is pushed. Otherwise
nothing changes. (`foo.style` and `.style(x)` therefore never enter style mode.)

### 4.4 Style mode

Style mode tokenizes a CSS-like language ([§16](#16-style-blocks)). It differs from code mode as follows:

- **Identifiers** may contain `-` after the first character: `sans-serif`, `nth-child`, `space-between`.
  They are always `Ident`; there are no keywords in style mode (`not`, `and` are `Ident`). K spells
  keyword values in camel case (`sansSerif`); a hyphenated keyword in a value still lexes as one `Ident`
  so that the checker can report it (K3214, [06-styles.md](../design/06-styles.md#keyword-values)).
- **Negative numbers**: `-` immediately followed by a digit, or by `.` and a digit, starts a numeric
  literal (`-4px` is one `UnitLit`, `-1` is one `IntLit`), as in CSS. `100% - 2px` (with spaces) is
  `UnitLit Minus UnitLit`. `-.5` is a `FloatLit` and `-.5em` a `UnitLit` (style mode only; `.5` alone
  is still `Dot IntLit`).
- **Punctuation** is limited to `( ) [ ] { } , : ; . & > + ~ * / ! = - % @`. `::` is two `Colon` tokens.
  Any other character (`?`, `<`, `|`, `$`, ...) is an unexpected character (K0001).
- Numbers, units, colors (`#fff`), strings (with interpolation) and comments (`//`, `///`, `/* */`) are
  lexed as in code mode.

Style mode is a small state machine over **statements**. A statement starts after the opening `{` of the
style block, after any `{`, `}` or `;`, and at the start of the block. At a statement start (skipping
trivia):

| First character | Statement | Meaning of `{` in this statement |
|---|---|---|
| `[A-Za-z_]` | **declaration** (`padding: .md;`) | `InterpStart`: an interpolation in code mode |
| anything else (`.`, `&`, `:`, `*`, `[`, `>`, `@`, ...) | **selector or at-rule prelude** | `LBrace`: opens a nested block (depth + 1) |

In every state:

- `;` is `Semicolon` and starts a new statement.
- `}` is `RBrace`. At depth 0 it closes the style block, pops the style frame and returns to the
  enclosing mode; otherwise it decrements the depth and starts a new statement.

**Keyframes.** A statement that starts with `@` directly followed by the word `keyframes` (not a
longer word such as `keyframes-x`) marks the block its `{` opens as a *keyframes block*. At a statement
start directly inside a keyframes block, a letter starts a **selector**, not a declaration, so `from {`
and `to {` open nested blocks like `50% {` does. Inside those nested blocks statements are declarations
again. The keyframes block ends at its matching `}`; a `;` or `}` before its `{` (`@keyframes x;`)
cancels it. Keyframes blocks nested in `@media` work at any depth.

Because a statement that starts with a letter is always a declaration (outside keyframes blocks), element selectors such as
`button { ... }` are not selectors in K ([RA-8](#ra-8-no-element-selectors)): the lexer treats `button` as
a property name, and the parser reports K0116. An interpolation that opens in a declaration **before its
`:`** (only this element-rule shape) also ends the statement when it closes, so the statement after
`button { color: red; }` starts fresh; an interpolation in a value (`padding: {x} {y};`) does not.

### 4.5 End of file

At end of file every open frame simply ends. Unterminated strings report K0002 (lexer); unclosed braces,
including an unclosed style block, report K0101 (parser).

---

## 5. Newlines and termination

K has no statement terminator symbol. **Newlines end statements, members and items**, with these rules:

**N1. Newlines inside `(` `)`, `[` `]` and generic `<` `>` are insignificant.** This holds at any depth
until a `{` is opened inside them, where rules N2–N4 apply again.

**N2. A newline is significant only where the current construct could end.** If the grammar requires
more tokens (after `func f`, after `let x =`, after a binary operator, after `,`, inside a declaration
header before its required body), the newline is skipped like whitespace.

**N3. Where the construct could end, a newline ends it, unless the next token is a continuation token.**
Continuation tokens cannot start a statement, member or item, so continuing is never ambiguous:

| Continuation token | Example |
|---|---|
| `.` `?.` | chained calls and modifiers on the next line |
| binary operators except `-`: `+ * / % == != < <= > >= ?? ..< ... and or` | `let ok = a`<br>`    and b` |
| `=` and the compound assignments | `x`<br>`    = 5` |
| `else` `catch` | `}`<br>`else {` |
| `where` `requires` `implements` `throws` `async` `->` `=>` `in` | clauses wrapped onto the next line |
| `key`, `at` (where the grammar allows them) | `for x in xs`<br>`    key x.id {` |
| `{` **where the grammar requires a block or body** | `if ready`<br>`{` |
| `)` `]` `}` `,` `:` `?` | closing and separating tokens |

`-` is not a continuation token: a line starting with `-` starts a new statement with a unary minus.
Break long subtractions after the operator, or use parentheses.

A binary operator at the start of a line continues the expression only when a token that can start an
operand follows it. A stray line such as `+ +` is therefore not glued to the statement above; it is
reported on its own (K0126 in a member list, K0122 in a block).

**N4. Postfix operations must start on the same line as their operand.** A call's `(`, an index's `[`
and a trailing lambda's `{` must be on the same line as the end of the callee. On the next line, `(`
starts a parenthesized expression statement, `[` a list literal, and `{` a lambda expression.

Consequences:

```k
paragraph { "Nothing here yet." }
    .class(.muted)                 // continues: leading `.`

let inStock = products
    .filter { product => product.stock > 0 }

if items.isEmpty {
    ...
}
else {                             // continues: `else` (the formatter moves it up)
    ...
}

return                             // bare return: `return` can end here...
    value                          // ...so this is a separate (unreachable) statement
```

**Semicolons.** `;` in code mode is K0103 (fix: delete it). For recovery it is treated as a statement
separator.

**Two statements on one line** (`let a = 1 let b = 2`) report K0102 at the start of the second, with a
fix inserting a newline.

---

## 6. Contextual keywords

The parser treats an `Ident` as a contextual keyword, and remaps its kind, only in these positions:

| Word(s) | Position | Followed by |
|---|---|---|
| `component` `page` `layout` `service` `protocol` `entity` `repository` `theme` `app` `config` `migration` | start of an item or member, after annotations and modifiers | `IDENT` |
| `test` | start of an item or member | `IDENT` or a string |
| `view` `loading` `metadata` | start of a member | `{` |
| `failed` | start of a member | `(` or `{` |
| `variant` | start of a member | `IDENT` |
| `matches` | in a `variant` header, after the name | an expression |
| `at` | in a `page` header, after the name and any `requires` clause | a string |
| `key` | in a `for` statement, after the iterable expression | an expression |
| `proto` | after `import` | a string |

Everywhere else these words are ordinary identifiers: `Analytics.send(page, ...)`, `let key = ...`,
`Cache.set(key, value)` and `let view = ...` are all valid.

The hard keyword `use` starts a `UseDecl` only at member level. In an expression, `use` directly followed
by `(` is a `NameExpr` (the token keeps the kind `UseKw`), so the common helper call `use(x)` parses as a
call ([RA-33](#ra-33-use-as-a-function-name)).

---

## 7. Files and imports

```ebnf
SourceFile ::= lines( Import | item )
Import     ::= 'import' ( ImportPath | 'proto' StringLit )
ImportPath ::= path_word ( ( '.' | '/' | '-' ) path_word )*     (* no trivia inside the path *)
path_word  ::= IDENT | keyword | contextual_keyword
```

- Imports must come before the first item (K0111 otherwise).
- An import names exactly one declaration: `import shop.catalog.Product`. Package paths may contain a host
  name, slashes and hyphens: `import github.com/acme/my-charts.LineChart`.
- `import proto "billing/v1/invoice.proto"` imports Protobuf definitions as K types.

```ebnf
item ::= ComponentDecl | PageDecl | LayoutDecl | ServiceDecl | ProtocolDecl | EntityDecl
       | RepositoryDecl | StructDecl | EnumDecl | ClassDecl | InterfaceDecl | ThemeDecl
       | AppDecl | ConfigDecl | MigrationDecl | ExtendDecl | TestSuiteDecl | TestCaseDecl | FuncDecl
       | FieldDecl
```

A top-level `FieldDecl` must use `let` (K0131 for `var`).

---

## 8. Declarations

Every declaration starts with the same prefix:

```ebnf
prefix ::= Annotation* ( 'public' | 'private' )? 'static'?
```

Annotations always come first (K0121 if an annotation follows a modifier, with a fix that moves it).
Repeating or combining modifiers (`public public`, `public private`) is K0127. Which modifiers are
meaningful for which declaration is checked by semantic analysis, not the parser.

```ebnf
ComponentDecl  ::= prefix 'component'  IDENT GenericParamList? MemberList
PageDecl       ::= prefix 'page'       IDENT RequiresClause? AtClause? MemberList
LayoutDecl     ::= prefix 'layout'     IDENT RequiresClause? MemberList
ServiceDecl    ::= prefix 'service'    IDENT ImplementsClause? MemberList
ProtocolDecl   ::= prefix 'protocol'   IDENT GenericParamList? RequiresClause? MemberList
EntityDecl     ::= prefix 'entity'     IDENT ImplementsClause? MemberList
RepositoryDecl ::= prefix 'repository' IDENT ForClause MemberList
StructDecl     ::= prefix 'struct'     IDENT GenericParamList? ImplementsClause? WhereClause? MemberList
EnumDecl       ::= prefix 'enum'       IDENT GenericParamList? ImplementsClause? WhereClause? MemberList
ClassDecl      ::= prefix 'class'      IDENT GenericParamList? ImplementsClause? WhereClause? MemberList
InterfaceDecl  ::= prefix 'interface'  IDENT GenericParamList? ImplementsClause? WhereClause? MemberList
ThemeDecl      ::= prefix 'theme'      IDENT MemberList
AppDecl        ::= prefix 'app'        IDENT MemberList
ConfigDecl     ::= prefix 'config'     IDENT ImplementsClause? MemberList
MigrationDecl  ::= prefix 'migration'  IDENT MemberList
ExtendDecl     ::= prefix 'extend' GenericParamList? Type WhereClause? MemberList
FuncDecl       ::= prefix 'func' IDENT GenericParamList? ParamList 'async'? ThrowsClause? ReturnType?
                   WhereClause? Block?
FieldDecl      ::= prefix ( 'let' | 'var' ) IDENT ( ':' Type )? ( '=' Expr | Block )?
```

Notes:

- Clause order is fixed and reads as prose: `page Admin requires Authenticated, Role(.admin) at "/admin"`.
- A `FuncDecl` without a body is a requirement (in an interface). Semantic analysis rejects bodiless
  functions elsewhere.
- A `FieldDecl` with a `Block` is a computed property: `var retries: RetryPolicy { RetryPolicy(attempts:
  self) }`. The block's last expression is its value.
- `interface A implements B` declares that every implementation of `A` also implements `B`.
- `extend<T> List<T> where T implements Priced { ... }` extends a generic type
  ([RA-16](#ra-16-extending-generic-types)).

```ebnf
MemberList ::= '{' lines( member ) '}'
```

---

## 9. Members

The parser accepts every member kind inside every `MemberList`; semantic analysis reports members that a
kind of declaration does not allow (for example a `view` in a `struct`). This keeps the parser simple and
its recovery uniform.

```ebnf
member ::= FieldDecl | FuncDecl | InitDecl | RpcDecl | EnumCase | UseDecl | ThemeToken | ThemeOverride
         | ThemeVariant
         | ViewBlock | StyleBlock | LoadingBlock | FailedBlock | MetadataBlock
         | TestSuiteDecl | TestCaseDecl

InitDecl      ::= prefix 'init' ParamList ThrowsClause? Block
RpcDecl       ::= prefix 'rpc' IDENT GenericParamList? ParamList 'async'? ThrowsClause? ReturnType?
                  RequiresClause? WhereClause? Block?
EnumCase      ::= prefix 'case' IDENT ( '(' ( CaseField ( ',' CaseField )* ','? )? ')' )?
CaseField     ::= ( IDENT ':' )? Type ( '=' Expr )?
UseDecl       ::= 'use' ( Type | Expr ) 'for' Type ( 'in' Expr ( ',' Expr )* )?   (* see below *)
ThemeToken    ::= IDENT IDENT ':' Type '=' Expr
ThemeVariant  ::= 'variant' IDENT ( 'matches' expr_nt )? '{' lines( ThemeOverride ) '}'
ThemeOverride ::= IDENT '=' Expr
ViewBlock     ::= 'view' Block
LoadingBlock  ::= 'loading' Block
FailedBlock   ::= 'failed' ( '(' Pattern ')' )? Block
MetadataBlock ::= 'metadata' Block
StyleBlock    ::= 'style' '{' css_item* '}'                (* see §16 *)
```

- Each enum case has its own `case` keyword. `case a, b` in an enum body is K0130 (fix: split into lines).
- An `RpcDecl` without a body declares an RPC in a `protocol`; with a body it implements one in a
  `service ... implements Protocol`. The signature reads `rpc name(params) async throws E -> T requires
  Guards { ... }` ([RA-1](#ra-1-position-of-requires-in-rpc-signatures)).
- A protocol's and an rpc's generic parameters (and an rpc's `where` clause) parse only so that semantic
  analysis can say why neither can be generic (K5514).
- `ThemeToken` declares a token: `color primary: Color = #3B82F6`. The first word is the token category.
- `ThemeOverride` assigns a token declared elsewhere (by a library such as `k.charts`, or by the theme
  for a variant): `chartGrid = #E2E8F0`. It is valid at the top level of a theme and inside variants
  ([07-themes-and-units.md](../design/07-themes-and-units.md#library-tokens)). Like every member, the
  parser accepts it in any body; semantic analysis rejects it outside a theme.
- `variant dark matches .prefersDark { ... }` applies the variant through a media query until the app
  chooses one explicitly. The condition is an `expr_nt` (its `{` opens the body), a direct child of the
  `ThemeVariant` before its overrides.
- `ConfigDecl` ([25-config-and-secrets.md](../design/25-config-and-secrets.md)) and `MigrationDecl`
  ([19-persistence.md](../design/19-persistence.md#migration-files)) have no header clauses; their
  members (`@Env` fields, `func up`) are ordinary fields and functions.
- `UseDecl` binds an implementation: `use StripePayments for PaymentProvider in .production`. It appears in
  `app` and `test` bodies.
- **The operand of `use`** (before `for`) is a `Type` or an `Expr`. It is an `Expr` exactly when
  1. it cannot start a type but can start an expression (`use .sample for C`, `use 42 for C`), or
  2. it starts with a dotted path of words (`a.b.C.d`) that is followed on the same line by `(` or `{`
     (a call: `use StripeWebhookConfig(signingSecret: s) for StripeWebhookConfig`), or
  3. that path has a segment after its first capitalized segment that is not a capitalized identifier
     (a member of a type: `use StripeWebhookConfig.sample for StripeWebhookConfig`).

  Otherwise it is a `Type`: `Fake`, `fakes.FakeApi`, `k.test.MemoryStorage`, `Outer.Inner`,
  `Repository<User>`, `(Fake)`. The `UseDecl` then holds the operand node directly, before the `for`
  token: `(UseDecl use (MemberExpr (NameExpr Config) sample) for (PathType Config))`. Semantic analysis
  allows an `Expr` operand only in a `test` body whose target is a `config` type
  ([25-config-and-secrets.md](../design/25-config-and-secrets.md#tests)); see [RA-35](#ra-35-the-operand-of-use).

---

## 10. Clauses, parameters, arguments and annotations

```ebnf
GenericParamList ::= '<' GenericParam ( ',' GenericParam )* ','? '>'
GenericParam     ::= IDENT
WhereClause      ::= 'where' WherePredicate ( ',' WherePredicate )*
WherePredicate   ::= IDENT 'implements' Type
ImplementsClause ::= 'implements' Type ( ',' Type )*
RequiresClause   ::= 'requires' Guard ( ',' Guard )*
Guard            ::= IDENT ( '.' IDENT )* ArgList?
AtClause         ::= 'at' StringLit
ForClause        ::= 'for' Type
ThrowsClause     ::= 'throws' Type
ReturnType       ::= '->' Type

ParamList ::= '(' ( Param ( ',' Param )* ','? )? ')'
Param     ::= Annotation* ( name | '_' )? IDENT ':' Type ( '=' Expr )?

Annotation ::= '@' IDENT ArgList?          (* the '(' must be on the same line as the name *)
ArgList    ::= '(' ( Arg ( ',' Arg )* ','? )? ')'
Arg        ::= ( name ':' )? ( BindExpr | WhenExpr | Expr )
BindExpr   ::= 'bind' Expr
WhenExpr   ::= Expr 'when' Expr
```

- **Annotation names** are any word (`@State`, `@page`, `@default`) except the keywords that start a
  declaration (`let`, `var`, `func`, `struct`, `enum`, `class`, `interface`, `extend`, `init`, `rpc`,
  `case`, `use`, `import`, `public`, `private`, `static`, `style`): `@ let x: Int` is a missing name
  (K0100 at `let`) followed by a field.
- **Generic constraints** are written only in `where` clauses (`func cheapest<T>(items: List<T>) -> T?
  where T implements Priced`).
- **`throws` must name a type** (K0129).
- **Parameters** have an optional external label before the name, as in Swift: `func reviews(for product:
  ProductId)` is called as `reviews(for: id)`; `_` makes the parameter positional. With a single word,
  label and name are the same. Labels may be keywords. The rules that match arguments to parameters belong
  to semantic analysis ([RA-5](#ra-5-argument-labels)).
- **`bind`** is allowed only directly as an argument value (K0110 elsewhere).
- **`when`** is allowed only as an argument value or a list element (K0109 elsewhere).
- `AtClause` strings may interpolate route parameters: `at "/product/{id}/{tab}"`
  ([RA-12](#ra-12-route-templates-use-interpolation)).

---

## 11. Types

```ebnf
Type           ::= OptionalType | PathType | TupleType | FuncType | ParenType
OptionalType   ::= Type '?'
PathType       ::= IDENT ( '.' IDENT )* GenericArgList?
GenericArgList ::= '<' Type ( ',' Type )* ','? '>'
TupleType      ::= '(' Type ',' Type ( ',' Type )* ','? ')'
FuncType       ::= '(' ( Type ( ',' Type )* ','? )? ')' 'async'? ThrowsClause? ReturnType
ParenType      ::= '(' Type ')'
```

- `?` binds to the type immediately before it: `(CartItem) -> View?` is a function returning an optional
  view; an optional function is `((CartItem) -> View)?`.
- `T??`, `T? ?` and longer runs of `?` are K0120, reported once per type; the parser builds a single
  `OptionalType` holding every `?` token.
- A parenthesized list followed by `async`, `throws` or `->` is a `FuncType`; with two or more elements
  and no arrow it is a `TupleType`; with one element and no arrow it is a `ParenType`.

---

## 12. Statements

```ebnf
Block ::= '{' lines( stmt ) '}'

stmt ::= LetStmt | AssignStmt | ReturnStmt | ThrowStmt | BreakStmt | ContinueStmt | GuardStmt
       | WhileStmt | ForStmt | DoStmt | ExpectStmt | ExpectThrowsStmt | Expr

LetStmt          ::= ( 'let' | 'var' ) IDENT ( ':' Type )? ( '=' Expr )?
AssignStmt       ::= Expr assign_op Expr
assign_op        ::= '=' | '+=' | '-=' | '*=' | '/=' | '%='
ReturnStmt       ::= 'return' Expr?          (* the value must start on the same line *)
ThrowStmt        ::= 'throw' Expr
BreakStmt        ::= 'break'
ContinueStmt     ::= 'continue'
GuardStmt        ::= 'guard' conditions 'else' Block
WhileStmt        ::= 'while' conditions Block
ForStmt          ::= 'for' 'await'? for_pattern 'in' expr_nt KeyClause? Block
KeyClause        ::= 'key' expr_nt
DoStmt           ::= 'do' Block CatchClause+
CatchClause      ::= 'catch' ( 'unknown' | Pattern ( ',' Pattern )* MatchGuard? ) Block
ExpectStmt       ::= 'expect' Expr
ExpectThrowsStmt ::= 'expect' 'throws' Pattern Block

conditions   ::= condition ( ',' condition )*
condition    ::= LetCondition | expr_nt
LetCondition ::= 'let' IDENT ( '=' expr_nt )?
for_pattern  ::= IdentPattern | TuplePattern | WildcardPattern
```

- An expression statement is the expression node itself; there is no wrapper node.
- Assignment is a statement, never an expression. The parser reads an expression and, if an assignment
  operator follows, wraps both sides in an `AssignStmt`. A second assignment operator on the same line
  (`a = b = c`, also after `let p = q`) is K0100 "assignments cannot be chained" at that operator; the
  operator and its value become an `ErrorNode` inside the statement.
- `if let footer` is shorthand for `if let footer = footer`.
- `guard`'s `else` block must exit (semantic analysis).
- `for await x in stream { }` reads a `Stream` ([29-server-streaming.md](../design/29-server-streaming.md));
  the `await` token is a direct child of the `ForStmt`. That it appears only in `async` functions and
  takes no `key` clause are semantic rules.
- `expr_nt` is an expression with **no trailing lambda at its top level** ([§13.4](#134-trailing-lambdas))
  and **no `else` block on a top-level `try`**: in `guard let x = try load() else { return }` the `else`
  belongs to the `guard`. Parenthesize a try-else inside a condition: `if (try load() else { 0 }) > 1 { }`.

---

## 13. Expressions

### 13.1 Productions

```ebnf
Expr ::= TryExpr | AwaitExpr | binary_expr

TryExpr    ::= 'try' Expr ( 'else' Block )?
AwaitExpr  ::= 'await' Expr
PrefixExpr ::= ( 'not' | '-' ) Expr
BinaryExpr ::= Expr binary_op Expr
CallExpr   ::= Expr ArgList LambdaExpr? | Expr LambdaExpr
MemberExpr ::= Expr ( '.' | '?.' ) name GenericArgList?   (* type arguments: RA-23 *)
IndexExpr  ::= Expr '[' Expr ']'

primary ::= Literal | StringLit | NameExpr | ImplicitMemberExpr | ParenExpr | TupleExpr
          | ListExpr | MapExpr | LambdaExpr | IfExpr | MatchExpr

Literal            ::= INT_LIT | FLOAT_LIT | UNIT_LIT | COLOR_LIT | 'true' | 'false' | 'nil'
StringLit          ::= STRING_START ( STRING_TEXT | STRING_ESCAPE | Interpolation )* STRING_END
Interpolation      ::= INTERP_START Expr INTERP_END
NameExpr           ::= IDENT GenericArgList? | 'self'    (* type arguments: RA-23 *)
ImplicitMemberExpr ::= '.' name
ParenExpr          ::= '(' Expr ')'
TupleExpr          ::= '(' Expr ',' Expr ( ',' Expr )* ','? ')'
ListExpr           ::= '[' ( list_element ( ',' list_element )* ','? )? ']'
list_element       ::= WhenExpr | Expr
MapExpr            ::= '[' ':' ']' | '[' MapEntry ( ',' MapEntry )* ','? ']'
MapEntry           ::= Expr ':' Expr
LambdaExpr         ::= '{' LambdaParamList? lines( stmt ) '}'
LambdaParamList    ::= ( LambdaParam ( ',' LambdaParam )*
                       | '(' ( LambdaParam ( ',' LambdaParam )* ','? )? ')' ReturnType? ) '=>'
LambdaParam        ::= IDENT ( ':' Type )?      (* types only in the parenthesized form *)
IfExpr             ::= 'if' conditions Block ( 'else' ( IfExpr | Block ) )?
MatchExpr          ::= 'match' expr_nt '{' lines( MatchArm ) '}'
MatchArm           ::= 'case' Pattern ( ',' Pattern )* MatchGuard? Block | 'default' Block
MatchGuard         ::= 'where' expr_nt
```

A `[` starts a `MapExpr` when it is followed by `:` `]`, or when its first element is followed by `:`.
`[]` is an empty `ListExpr`; `[:]` is an empty `MapExpr`.

A `{` in expression position is a lambda. It has a parameter list when it starts with `IDENT (, IDENT)*
=>` or with a parenthesized list followed by `=>` or `->`. The lookahead is unbounded in length but
stops at the first `{` or `}` (parameter names and types contain none), so lists of any length are
recognized and the total lookahead over a file stays linear. Lambda parameters are always
named; there is no implicit `it`.

There is no ternary operator: `a ? b : c` is K0107 with a hint to use `if`. There is no `!`, `&&` or `||`
(K0106, K0104, K0105, each with a machine-applicable fix). For recovery the parser treats them as `not`,
`and` and `or` with the same precedence, keeping the original token in the tree: `a && b` builds
`(BinaryExpr (NameExpr a) && (NameExpr b))`.

### 13.2 Precedence and associativity

From lowest to highest:

| Level | Operators | Associativity | Node |
|---|---|---|---|
| 1 | `try` `await` (prefix) | right; covers the whole expression to its right | `TryExpr`, `AwaitExpr` |
| 2 | `or` | left | `BinaryExpr` |
| 3 | `and` | left | `BinaryExpr` |
| 4 | `not` (prefix) | right | `PrefixExpr` |
| 5 | `==` `!=` `<` `<=` `>` `>=` | none | `BinaryExpr` |
| 6 | `??` | right | `BinaryExpr` |
| 7 | `..<` `...` | none | `BinaryExpr` |
| 8 | `+` `-` | left | `BinaryExpr` |
| 9 | `*` `/` `%` | left | `BinaryExpr` |
| 10 | `-` (prefix) | right | `PrefixExpr` |
| 11 | call `( )`, trailing lambda, `.` `?.` member, `[ ]` index | left | `CallExpr`, `MemberExpr`, `IndexExpr` |

- **Non-associative chains** (`a < b < c`, `a..<b..<c`) are K0113. The parser builds them
  left-associatively for recovery.
- `not a == b` is `not (a == b)`; `not a and b` is `(not a) and b`.
- `-5.minutes` is `-(5.minutes)`; `-x.y` is `-(x.y)`.
- `a ?? b ?? c` is `a ?? (b ?? c)`.
- **`try` and `await` must start the expression they cover.** `a + try b` is K0114; the parser still
  builds `a + (try b)`. Inside arguments, list elements and parentheses they are fine:
  `f(try g())`.
- `try await x` is `TryExpr(AwaitExpr(x))`. `try x else { ... }` attaches the `else` block to the `try`.

### 13.3 Postfix chains

```k
Router.navigate(to: .product(id: product.id).tab(.reviews))
```

`.product(id: ...)` is `CallExpr(ImplicitMemberExpr product, ArgList)`, and `.tab(.reviews)` is a call on
that: `CallExpr(MemberExpr(CallExpr(...), tab), ArgList)`. Chains may continue on following lines with a
leading `.` (rule N3).

### 13.4 Trailing lambdas

A `{` immediately after an expression on the same line is a trailing lambda: it adds a `LambdaExpr` to a
`CallExpr`. `box { ... }` is a call of `box` with only a trailing lambda; `heading(level: 2) { title }`
has both. A call has at most one trailing lambda; further named slots are chained (`.row { item => ... }`).

**No trailing lambdas in `expr_nt` positions.** In `if`, `guard` and `while` conditions, the `for`
iterable and `key`, the `match` scrutinee and match/catch guards, a `{` ends the expression instead,
because it opens the statement's block. `if items.isEmpty { ... }` therefore parses as intended. A `{` at the very start of such a position is
never a lambda either: `if { go() }` is a missing condition (K0122 at the `{`) followed by the block. To
call a function with a trailing lambda in such a position, parenthesize it: `if (items.contains { i => i.isNew })
{ ... }`. Inside parentheses, brackets and argument lists the restriction is lifted.

---

## 14. Patterns

```ebnf
Pattern         ::= WildcardPattern | BindingPattern | LiteralPattern | RangePattern
                  | EnumCasePattern | TuplePattern
WildcardPattern ::= '_'
BindingPattern  ::= 'let' IDENT
IdentPattern    ::= IDENT                                   (* only in `for` *)
LiteralPattern  ::= '-'? ( INT_LIT | FLOAT_LIT | UNIT_LIT ) | StringLit | 'true' | 'false' | 'nil'
RangePattern    ::= LiteralPattern ( '..<' | '...' ) LiteralPattern
EnumCasePattern ::= ( '.' name | IDENT ( '.' IDENT )* '.' name )
                    ( '(' ( pattern_arg ( ',' pattern_arg )* ','? )? ')' )?
pattern_arg     ::= PatternField | Pattern
PatternField    ::= IDENT ':' Pattern
TuplePattern    ::= '(' Pattern ',' Pattern ( ',' Pattern )* ','? ')'
```

- Bindings always use `let`: `case .loaded(let product)`, `catch .outOfStock(let available)`. There is no
  `var` pattern.
- A bare identifier in a `match` or `catch` pattern is K0115 (write `let x` or `.x`); the parser builds an
  `IdentPattern` for recovery.
- Qualified case patterns name the enum: `expect throws CartError.outOfStock { ... }`.
- A catch-all is `default` in `match`, and `catch let error` or `catch _` in `do`.
- `catch unknown {` is the catch-all for cases added later ([09-errors.md](../design/09-errors.md#cases-added-later-catch-unknown)). `unknown` is a contextual keyword only between `catch` and `{`; the parser builds a `WildcardPattern` holding the `unknown` identifier token (`WildcardPattern::is_unknown`), so later passes treat it as `catch _` except where sema checks it.

---

## 15. Views, loading, failed and metadata blocks

`view`, `loading`, `failed` and `metadata` bodies are ordinary `Block`s. There is no separate view
grammar: inside a view, each **expression statement is a view node**, and the statement forms map to
view control flow:

| Syntax in a view | Meaning |
|---|---|
| `"text {x}"` | a text node |
| `paragraph { ... }`, `heading(level: 2) { title }` | an element (lowercase) with children |
| `CartRow(item: item)` | a component (capitalized) |
| `content`, `row(item)` | a slot, or a scoped slot call |
| `.class(...)`, `.on(click: f)`, `.disabled(x)` on the next line | attributes, events and modifiers (postfix chain) |
| `if` / `else`, `if let` | conditional nodes |
| `for item in items key item.id { ... }` | a keyed list |
| `match x { case ... }` | branching on an enum |
| `let x = ...` | a local constant |

Which names are elements, which attributes exist and which statements are allowed in a view are
semantic rules (see `docs/design/05-views.md`). The `key` clause is accepted on every `for` by the parser;
semantic analysis requires it in views and rejects it elsewhere.

`metadata` blocks are blocks of calls (`title("...")`, `canonical(.product(id: id))`).

---

## 16. Style blocks

Style blocks are lexed in style mode ([§4.4](#44-style-mode)).

```ebnf
StyleBlock ::= 'style' '{' css_item* '}'
css_item   ::= StyleRule | KeyframesRule | AtRule | Declaration

StyleRule        ::= Selector ( ',' Selector )* '{' css_item* '}'
Selector         ::= CompoundSelector ( ( '>' | '+' | '~' )? CompoundSelector )*
CompoundSelector ::= simple_selector+                  (* no trivia between simple selectors *)
simple_selector  ::= ClassSelector | NestingSelector | PseudoClass | PseudoElement
                   | AttributeSelector | UniversalSelector | TypeSelector
ClassSelector     ::= '.' IDENT
NestingSelector   ::= '&'
PseudoClass       ::= ':' IDENT ( '(' ( Selector ( ',' Selector )* | CssValue ) ')' )?
PseudoElement     ::= ':' ':' IDENT                    (* the colons are adjacent *)
AttributeSelector ::= '[' IDENT ( '=' ( StringLit | IDENT ) )? ']'
UniversalSelector ::= '*'
TypeSelector      ::= IDENT                            (* always an error: K0116 *)

AtRule      ::= '@' IDENT AtPrelude? ( '{' css_item* '}' | ';' )   (* any name but `keyframes` *)
KeyframesRule    ::= '@' 'keyframes' IDENT '{' KeyframeBlock* '}'  (* no trivia after '@' *)
KeyframeBlock    ::= KeyframeSelector ( ',' KeyframeSelector )* '{' css_item* '}'
KeyframeSelector ::= IDENT | UNIT_LIT                     (* `from`, `to`, `50%` *)
AtPrelude   ::= ( IDENT | MediaFeature )+
MediaFeature ::= '(' IDENT ( ':' CssValue )? ')'

Declaration   ::= IDENT ':' CssValue ( ',' CssValue )* ';'?
CssValue      ::= css_component+
css_component ::= Literal | StringLit | IDENT | CssFunction | Interpolation | css_expr
                | '+' | '-' | '*' | '/'
CssFunction   ::= IDENT '(' CssValue ( ',' CssValue )* ')'      (* no trivia before '(' *)
css_expr      ::= ImplicitMemberExpr ( ( '.' name ) | ArgList )*  (* a K postfix chain; builds
                                                                     MemberExpr / CallExpr nodes *)
```

- **Descendant combinator.** Two compound selectors separated only by trivia are descendants:
  `.a .b`. Whitespace inside a compound selector is therefore significant to the parser: `.a:hover`
  is one compound, `.a :hover` is two.
- **Pseudo-class arguments.** For `:not`, `:is`, `:where` and `:has` the argument is a selector list;
  for every other pseudo-class (`:nth-child(2n + 1)`) it is a `CssValue`. Unit suffixes are not validated
  inside pseudo-class arguments.
- **Declarations** must end with `;` unless they are the last item of their block (K0117 with a fix
  inserting `;`). A declaration directly in the style block, outside any rule, is K0133.
- **Values.** Bare identifiers are CSS keywords (`solid`, `hidden`, `sansSerif`). K values start with
  `.` (implicit members and their chains: `.md`, `.primary.opacity(0.8)`) or are interpolated with `{ }`
  (reactive values: `{accent}`). Theme tokens and static values never need braces.
- **Commas** separate the values of a declaration: `fontFamily: "Inter", sansSerif;` has two `CssValue`s.
- `!important` is K0118. `#id` selectors are K0132. Both are wrapped in an `ErrorNode`.
- Media feature names are camelCase like properties: `@media (maxWidth: 600px)`.
- **Keyframes.** `@keyframes fade { from { opacity: 0; } 50% { opacity: 0.5; } to { opacity: 1; } }` is a
  `KeyframesRule` ([06-styles.md](../design/06-styles.md#animations-keyframes)); `keyframes` stays an
  `Ident` token (style mode has no keywords) and dumps as an atom. The lexer makes `from` and `to`
  selectors there (§4.4). The parser accepts any `IDENT` or `UNIT_LIT` as a keyframe selector and any
  style item inside a keyframe block; semantic analysis requires `from`, `to` or a percentage from `0%`
  to `100%`, and declarations only. A keyframe selector without `{` is K0100 at the next token, and
  recovery skips to the end of the statement (`;` or `}`) or to a keyframe selector that starts the
  next line. Anything else where a keyframe selector is expected is K0100, skipped up to the next
  keyframe selector or `}`. A missing name is K0100 at the token after `keyframes`.

---

## 17. Tests

```ebnf
TestSuiteDecl ::= prefix 'test' IDENT MemberList
TestCaseDecl  ::= prefix 'test' StringLit Block
```

A test file (`CartService.test.k`) contains `test Subject { ... }` suites holding `use` overrides,
injected fields, helpers and `test "description" { ... }` cases. A bare `test "..." { }` is also allowed
at the top level. `expect` and `expect throws` are statements ([§12](#12-statements)).

```k
test CartService {
    use FakeCartApi for CartProtocol

    @Inject let cart: CartService

    test "out of stock surfaces a typed error" {
        expect throws CartError.outOfStock {
            try await cart.add(product: .soldOut)
        }
    }
}
```

---

## 18. Resolved ambiguities

Each entry records a question the design documents leave open and the answer this grammar uses, with the
principle it follows ([docs/design/01-principles.md](../design/01-principles.md)).

### RA-1. Position of `requires` in rpc signatures

The design marks this provisional. **Decision:** `requires` comes after the complete signature, before the
body: `rpc add(request: AddItemRequest) throws CartError -> CartItem requires Authenticated,
RateLimit(100.perMinute)`. This keeps the effects in their fixed order (`async throws E -> T`)
uninterrupted, matches the position of `requires` in `page` and `protocol` headers (after the name and
signature, before the body) and of `where` in functions, and reads as prose.

### RA-2. Logical operators are words

**Decision:** `and`, `or`, `not`. "Keywords over symbols" applies, and the design already rules out `!`.
`&&`, `||` and `!` are lexed so the parser can offer machine-applicable fixes (K0104–K0106). `!=` stays: it
is the universal comparison operator, like `==`.

### RA-3. Nil coalescing is `??`

Optionals need a default-value operator. **Decision:** `??`, from the `?`/`?.` family the design keeps
for optionals. Precedence between comparison and range, right-associative (as in Swift).

### RA-4. Percent literal versus remainder

`%` is the remainder operator (arithmetic symbols are universal). A number immediately followed by `%` is a
`Percent` literal, so `10%3` is `UnitLit IntLit` and a parse error; write `10 % 3`. The formatter always
puts spaces around binary operators other than the ranges `..<` and `...`
([56-formatter.md](../design/56-formatter.md#spaces)), so this never arises in formatted code.

### RA-5. Argument labels

The design shows labeled calls (`cart.add(product: p)`), positional calls (`products.find(id)`) and mixed
calls (`repo.add(product, for: id)`). **Decision:** parameters may declare an external label before the
name, as in Swift (`for productId: ProductId`, or `_` for positional). The parser accepts labeled and
unlabeled arguments anywhere; matching them to parameters is a semantic rule. Labels may be keywords
(`for:`, `in:`, `as:`).

### RA-6. Contextual keywords

Declaration kinds that are common English words (`page`, `layout`, `test`, `view`, `key`, ...) are
contextual, because the design uses some of them as ordinary names (`Analytics.send(page, ...)`,
`Cache.set(key, value)`). The Swift-core keywords (`struct`, `enum`, `class`, `interface`, `func`, ...)
are hard. See [§6](#6-contextual-keywords).

### RA-7. `style` is a hard keyword

`style` switches the lexer into style mode, which must be decidable without the parser. It is therefore
a hard keyword; it can still be used after `.` and as a label.

### RA-8. No element selectors

Classes are typed and scoped; K element names (`box`) differ from HTML tags anyway. **Decision:** style
blocks select classes, `&`, pseudo-classes and pseudo-elements, attributes and `*`, never elements or ids.
A statement starting with a letter is always a declaration (except directly inside a `@keyframes` block,
[RA-36](#ra-36-keyframes)), which keeps style lexing context-free. Elements are styled through classes:
`heading(level: 1) { title }.class(.title)`.

### RA-9. Comments in style blocks

`//` and `/* */` are comments in style blocks, as in K code. Consequently there is no raw `url(...)` token;
write `url("https://...")` with a string.

### RA-10. String escapes

Only `{{` escapes a brace, since `{ }` always means "a K expression goes here". `}` needs no escape. `\{`
is an error with a fix. The backslash escapes are `\n \r \t \0 \\ \" \u{...}`.

### RA-11. Strings are single-line

Strings cannot contain raw line breaks, and an interpolation inside a string cannot span lines. Multi-line
string literals are deferred.

### RA-12. Route templates use interpolation

`page ProductDetail at "/product/{id}/{tab}"` is an ordinary string whose interpolations must each be a
single `NameExpr` naming a `@Param` of the page (a semantic rule). This keeps one meaning for `{ }`.

### RA-13. No semicolons

Newlines end statements ([§5](#5-newlines-and-termination)). There is no way to put two statements on one
line ("one obvious way"); single-statement lambdas like `{ value -= step }` cover the common case.

### RA-14. Annotations first

Annotations precede modifiers: `@Inject private let api: CartProtocol`. Java orders them the same way.

### RA-15. The `app` declaration

The design says configuration goes in `App.k` but leaves its structure open. **Decision (matching
[23-app-structure.md](../design/23-app-structure.md)):** `App.k`, at the project
root next to `k.toml`, holds one `public app Name { ... }` declaration. Its members are `use` bindings and
a `metadata` block; annotations on it (`@Layout(Dashboard)`) set app-wide defaults. Interceptor syntax is
deferred.

### RA-16. Extending generic types

`extend<T> List<T> where T implements Priced { ... }`: generic parameters follow `extend`, as they follow
the name in other declarations.

### RA-17. `static`, `init` and computed properties

The design uses static members (`.sample`, `.empty`) and objects with constructors without showing their
declarations. **Decision:** `static let`/`static func` declare static members, `init(...)` declares an
initializer, and `var name: T { ... }` declares a computed property, as in Swift.

### RA-18. Values of blocks

When a block is used as a value (a lambda body, an `if`/`match` arm used as an expression, a computed
property), its last expression statement is the value. Functions with a return type use `return`. This
matches the design's examples and is enforced by semantic analysis.

### RA-19. `if let` and condition lists

`if let x { }` and `if let x = e { }` bind optionals; conditions may be combined with commas: `if let a,
let b, a.isReady { }`. `if case` does not exist: use `match`.

### RA-20. Exponents and units

`1e5` is a float; `1em` is a length. An `e` starts an exponent only when a digit (optionally signed)
follows.

### RA-21. ASCII identifiers

Identifiers are ASCII. K's naming rules are English, and ASCII avoids confusable characters and Unicode
normalization questions. Strings and comments may contain any Unicode text.

### RA-22. No tuple element access by index

`pair.0` is not supported (it collides with float lexing, as `t.0.1` shows). Use destructuring in `for` or
pattern matching.

### RA-23. Explicit generic arguments in expressions

`f<Int>(x)` is ambiguous with comparisons in general. Type arguments are normally inferred, or given by
annotating the binding (`let tags: Set<String> = Set(["new"])`), and may also be written after a name or a
member name: `decode<User>(json)`, `Jwt.verify<Claims>(token, …)`, `KeyName<AesGcmKey>(name: "vault")`,
`Sealed<Note>.parse(text)`, `names.map<Int> { name => name.count }`.

**Decision:** a `<` after a name in an expression starts a `GenericArgList` only when a lookahead over
the tokens (`types::type_args_ahead`) finds all of:

- the `<` directly after the name and directly before the first type, with no trivia;
- a balanced list of type tokens only (names, `.`, `,`, `<`, `>`, `?`, parentheses, `->`, `async`,
  `throws`), on one line, each type argument starting with an uppercase name;
- the closing `>` directly after the last type, followed directly by `(` or `.`, or by a trailing
  lambda's `{` on the same line.

Otherwise `<` is a comparison. Since comparisons don't chain (K0113), `a<B>(c)` was never valid as one;
the rule's job is the one real ambiguity, two comparisons as arguments, `f(a < b, c > (d))`, which stays
two arguments because the operators are spaced (and because `b` and `c` are lowercase). A generic
function used as a value with its arguments (`map(decode<User>)`) isn't written this way: give the type by
the expected type instead.

### RA-24. `bind` and `when` positions

`bind x` is only an argument value; `x when c` is only an argument value or list element. These are the
only places the design uses them, and restricting them keeps `when` out of the precedence table.

### RA-25. Bare identifiers in patterns

In `match` and `catch`, `case x` would be ambiguous between binding and comparing. **Decision:** binding
needs `let`, enum cases need a leading `.` or a qualified name, and a bare name is K0115.

### RA-26. One case per line in enums

`case a, b` in an enum body is K0130. Commas separate *patterns* in `match` arms (`case .idle, .loading`),
so using them for declarations too would make the two read alike.

### RA-27. `failed` binding

`failed(let error) { ... }` binds the error with an ordinary pattern. `failed { ... }` without a binding is
allowed.

### RA-28. Media queries

At-rule preludes support media types, `and`/`not`/`only` words and `(feature: value)` or `(feature)`.
Range syntax (`(width >= 600px)`) is deferred. Feature names are camelCase.

### RA-29. Doc comments

`///` comments document the declaration that follows. They are `DocComment` trivia; the parser attaches
them to the next item or member when building the tree. Block doc comments (`/** */`) do not exist.

### RA-30. Generic close before `=`

`List<Int>= []` lexes `>=`. Write a space (`List<Int> = []`); the formatter does.

### RA-31. Top-level declarations

Top-level functions and `let` constants are allowed (the design makes functions importable anywhere).
Top-level `var` is K0131: shared mutable state belongs in a service.

### RA-32. Adopted design proposals

The former design proposals are now part of `docs/design`. Those that add syntax fit this grammar
without structural changes:

- `config Name { @Env("X") let field: T = default }`
  ([25-config-and-secrets.md](../design/25-config-and-secrets.md)) and `migration Name { func up(...) }`
  ([19-persistence.md](../design/19-persistence.md)) are contextual declaration keywords with the usual
  `prefix KEYWORD IDENT MemberList` shape (`ConfigDecl`, `MigrationDecl`).
- `for await x in stream` ([29-server-streaming.md](../design/29-server-streaming.md)) is an optional
  `await` in `ForStmt`.
- `use <expression> for <Config>` in test bodies ([25-config-and-secrets.md](../design/25-config-and-secrets.md#tests))
  is the `Expr` form of `UseDecl`'s operand ([RA-35](#ra-35-the-operand-of-use)).
- Theme overrides at the top level of a theme and `variant name matches .condition`
  ([07-themes-and-units.md](../design/07-themes-and-units.md)) are described in [§9](#9-members).
- `catch let error` and `case let other` ([08-types.md](../design/08-types.md)) are already valid: they
  are `BindingPattern`s.
- `///` doc comments before annotations ([39-doc-comments.md](../design/39-doc-comments.md)) match
  [RA-29](#ra-29-doc-comments).

### RA-33. `use` as a function name

Tests and views call helpers named `use` (`use(next)`, `use(other)`). `use` stays a hard keyword, because
`use A for B` must be recognizable at member level, but in an expression `use` followed by `(` is a
`NameExpr`. `use` not followed by `(` cannot start an expression, so `use A for B` on the line after an
unfinished `UseDecl` is still a new binding.

### RA-34. Nesting limit

Recursive constructs (blocks, lambdas, member lists, style rules, parenthesized, bracketed and generic
lists, interpolations, prefix operators, right-associative `??` chains, ternaries, `else if` chains,
the return and thrown types of function types (`() -> () -> T`), and `if`/`match` expressions nested in
conditions and scrutinees (`if if a { } { }`)) may nest at most 400 levels. Deeper input is K0134, reported once at the first token beyond the limit;
the rest of that construct, up to its matching closing delimiter, is skipped into an `ErrorNode` without
recursion. This bounds the parser's stack use on every input (about 2 MiB in debug builds and 1 MiB in
release builds at the limit). Valid programs never come close.

### RA-35. The operand of `use`

Tests bind a config to a value with `use <expression> for <Config>`
([25-config-and-secrets.md](../design/25-config-and-secrets.md#tests)), while `App.k` and test groups
bind implementations with `use <Type> for <Type>`. **Decision:** `UseDecl` takes a `Type` or an `Expr`
before `for`, decided by a bounded lookahead over the operand's leading dotted path
([§9](#9-members)): a call (`Config(…)`), a member of a type (`Config.sample`) or anything that cannot
start a type makes it an `Expr`. The decision uses K's naming convention (types are capitalized,
modules and members are not), the one place the parser relies on it, because `Config.sample` and
`shop.Config` have the same token shape. Where the operand is allowed is a semantic rule.

### RA-36. Keyframes

CSS keyframe selectors `from` and `to` start with a letter, which would make them declarations in
style mode (RA-8). **Decision:** the lexer tracks the block opened by `@keyframes` and lexes a
letter at a statement start directly inside it as a selector (§4.4). This keeps the lexer independent of
the parser: the decision depends only on the characters `@keyframes` at a statement start. Keyframes are
referenced from properties as implicit members (`animationName: .fade`) and scoped like classes.

---

## 19. Syntax tree dump format

Parser golden tests (`tests/corpus/parser/**/*.ast`) compare a compact S-expression dump of the syntax
tree. The dump is derived mechanically from the tree, so it is fully determined by the grammar above.

### 19.1 Rules

1. **Nodes** become lists whose head is the node kind: `(ComponentDecl ...)`.
2. **Omitted tokens.** These tokens are left out, because the tree shape already conveys them:
   trivia (`Whitespace`, `Newline`, `LineComment`, `BlockComment`, `DocComment`), `(` `)` `[` `]` `{` `}`,
   `,` `;` `:` `.` `?` `@` `->` `=>`, the string quotes (`StringStart`, `StringEnd`), the interpolation
   braces (`InterpStart`, `InterpEnd`), and the `<` and `>` of `GenericParamList` and `GenericArgList`
   (elsewhere `<` and `>` are comparison operators or combinators and are kept). A string literal with
   no text dumps as `(StringLit)`.
3. **Strings.** Each maximal run of `StringText` and `StringEscape` tokens becomes **one quoted atom**
   holding the *decoded* value. The K source `"Line\n{{x}"` (text `Line`, escape `\n`, escape `{{`, text
   `x}`) dumps as `(StringLit "Line\n{x}")`; the `\n` inside the atom is the dump's own escape for the
   newline character. An interpolation splits the runs: `"a{b}c"` dumps as
   `(StringLit "a" (Interpolation (NameExpr b)) "c")`. Invalid escapes (already reported by the lexer) are
   kept verbatim: `"\q"` dumps as `(StringLit "\\q")`.
4. **Every other token** becomes an atom of its source text: identifiers (`count`), keywords (`var`,
   `component`), operators (`+=`, `?.`, `..<`), literals (`0`, `16px`, `#3B82F6`, `50%`) and error tokens.
   An atom is written bare unless its text contains whitespace, `(`, `)`, `"` or `;`, or is empty, in which
   case it is quoted.
5. Quoted atoms use the escapes `\\` `\"` `\n` `\r` `\t` and `\u{HEX}` for other control characters.

Because quoted and bare atoms differ, the string literal `"count"` (`(StringLit "count")`) is distinct from
the name `count` (`(NameExpr count)`).

### 19.2 Comparison

The harness parses the expected file and the actual dump as S-expressions and compares them
**structurally**: layout, indentation and `;` comments in `.ast` files are free. A failure prints both in
canonical layout (a list fits on one line if it is at most 100 columns wide at its indentation; otherwise
its leading atoms stay on the first line and every other element goes on its own line, indented two more
spaces). `K_BLESS=1` writes canonical layout.

### 19.3 Examples

A component:

```k
component Counter {
    @State var count: Int = 0

    func increment() {
        count += 1
    }
}
```

```lisp
(SourceFile
  (ComponentDecl component Counter
    (MemberList
      (FieldDecl (Annotation State) var count (PathType Int) = (Literal 0))
      (FuncDecl func increment (ParamList) (Block (AssignStmt (NameExpr count) += (Literal 1)))))))
```

A view with a chained modifier, an interpolated string and a binding argument:

```k
view {
    text { "{label}: {count}" }

    Stepper(value: bind quantity)
        .step(5)
}
```

```lisp
(ViewBlock view
  (Block
    (CallExpr (NameExpr text)
      (LambdaExpr
        (StringLit (Interpolation (NameExpr label)) ": " (Interpolation (NameExpr count)))))
    (CallExpr
      (MemberExpr
        (CallExpr (NameExpr Stepper) (ArgList (Arg value (BindExpr bind (NameExpr quantity)))))
        step)
      (ArgList (Arg (Literal 5))))))
```

Classes with `when`, and a lambda with a parameter:

```k
box { ... }.class([.panel, .expanded when isExpanded])
let names = users.map { user => user.name }
```

```lisp
(CallExpr (MemberExpr (CallExpr (NameExpr box) (LambdaExpr ...)) class)
  (ArgList (Arg (ListExpr (ImplicitMemberExpr panel)
                          (WhenExpr (ImplicitMemberExpr expanded) when (NameExpr isExpanded))))))
(LetStmt let names =
  (CallExpr (MemberExpr (NameExpr users) map)
    (LambdaExpr (LambdaParamList (LambdaParam user)) (MemberExpr (NameExpr user) name))))
```

(`...` stands for elided content in this document; it is not valid in `.ast` files.)

Control flow:

```k
let label = match status {
    case .active { "On" }
    case .shipped(let tracking) where tracking.isDelayed { "Late" }
    default { "Off" }
}
guard let user = session.user else { throw CheckoutError.notLoggedIn }
for item in items key item.id {
    CartRow(item: item)
}
let item = try await api.add(request: request)
```

```lisp
(LetStmt let label =
  (MatchExpr match (NameExpr status)
    (MatchArm case (EnumCasePattern active) (Block (StringLit "On")))
    (MatchArm case (EnumCasePattern shipped (BindingPattern let tracking))
      (MatchGuard where (MemberExpr (NameExpr tracking) isDelayed))
      (Block (StringLit "Late")))
    (MatchArm default (Block (StringLit "Off")))))
(GuardStmt guard (LetCondition let user = (MemberExpr (NameExpr session) user)) else
  (Block (ThrowStmt throw (MemberExpr (NameExpr CheckoutError) notLoggedIn))))
(ForStmt for (IdentPattern item) in (NameExpr items) (KeyClause key (MemberExpr (NameExpr item) id))
  (Block (CallExpr (NameExpr CartRow) (ArgList (Arg item (NameExpr item))))))
(LetStmt let item =
  (TryExpr try (AwaitExpr await
    (CallExpr (MemberExpr (NameExpr api) add) (ArgList (Arg request (NameExpr request)))))))
```

Declarations with clauses:

```k
page Admin requires Authenticated, Role(.admin) at "/admin" {}

protocol CartProtocol {
    rpc add(request: AddItemRequest) throws CartError -> CartItem requires RateLimit(100.perMinute)
}

enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
}
```

```lisp
(PageDecl page Admin
  (RequiresClause requires
    (Guard Authenticated)
    (Guard Role (ArgList (Arg (ImplicitMemberExpr admin)))))
  (AtClause at (StringLit "/admin"))
  (MemberList))
(ProtocolDecl protocol CartProtocol
  (MemberList
    (RpcDecl rpc add (ParamList (Param request (PathType AddItemRequest)))
      (ThrowsClause throws (PathType CartError))
      (ReturnType (PathType CartItem))
      (RequiresClause requires
        (Guard RateLimit (ArgList (Arg (MemberExpr (Literal 100) perMinute))))))))
(EnumDecl enum CartError (ImplementsClause implements (PathType Error))
  (MemberList
    (EnumCase case productNotFound)
    (EnumCase case outOfStock (CaseField available (PathType Int)))))
```

A style block:

```k
style {
    .summary {
        padding: .md;
        borderLeft: 4px solid {accent};
        fontFamily: "Inter", sansSerif;

        &:hover { backgroundColor: .surfaceHover; }

        @media (maxWidth: 600px) { padding: .sm; }
    }
}
```

```lisp
(StyleBlock style
  (StyleRule (Selector (CompoundSelector (ClassSelector summary)))
    (Declaration padding (CssValue (ImplicitMemberExpr md)))
    (Declaration borderLeft (CssValue (Literal 4px) solid (Interpolation (NameExpr accent))))
    (Declaration fontFamily (CssValue (StringLit "Inter")) (CssValue sansSerif))
    (StyleRule (Selector (CompoundSelector (NestingSelector &) (PseudoClass hover)))
      (Declaration backgroundColor (CssValue (ImplicitMemberExpr surfaceHover))))
    (AtRule media (AtPrelude (MediaFeature maxWidth (CssValue (Literal 600px))))
      (Declaration padding (CssValue (ImplicitMemberExpr sm))))))
```

More node shapes, for reference:

| Source | Dump |
|---|---|
| `import shop.catalog.Product` | `(Import import (ImportPath shop catalog Product))` |
| `@Effect(on: [userId])` | `(Annotation Effect (ArgList (Arg on (ListExpr (NameExpr userId)))))` |
| `@Prop let step: Int = 1` | `(FieldDecl (Annotation Prop) let step (PathType Int) = (Literal 1))` |
| `func f(for id: Id) async throws E -> T` | `(FuncDecl func f (ParamList (Param for id (PathType Id))) async (ThrowsClause throws (PathType E)) (ReturnType (PathType T)))` |
| `func f<T>() where T implements X {}` | `(FuncDecl func f (GenericParamList (GenericParam T)) (ParamList) (WhereClause where (WherePredicate T implements (PathType X))) (Block))` |
| `let row: (CartItem) -> View?` | `(LetStmt let row (FuncType (PathType CartItem) (ReturnType (OptionalType (PathType View)))))` |
| `List<Map<K, V>>` | `(PathType List (GenericArgList (PathType Map (GenericArgList (PathType K) (PathType V)))))` |
| `user?.name ?? "Anon"` | `(BinaryExpr (MemberExpr (NameExpr user) ?. name) ?? (StringLit "Anon"))` |
| `not a and b` | `(BinaryExpr (PrefixExpr not (NameExpr a)) and (NameExpr b))` |
| `xs[0]` | `(IndexExpr (NameExpr xs) (Literal 0))` |
| `[id1: 9.99]` / `[:]` / `[]` | `(MapExpr (MapEntry (NameExpr id1) (Literal 9.99)))` / `(MapExpr)` / `(ListExpr)` |
| `if done { "A" } else { "B" }` | `(IfExpr if (NameExpr done) (Block (StringLit "A")) else (Block (StringLit "B")))` |
| `if let footer { footer }` | `(IfExpr if (LetCondition let footer) (Block (NameExpr footer)))` |
| `try cart.add(p) else { return }` | `(TryExpr try (CallExpr (MemberExpr (NameExpr cart) add) (ArgList (Arg (NameExpr p)))) else (Block (ReturnStmt return)))` |
| `{ (p: Product) -> Money => p.price }` | `(LambdaExpr (LambdaParamList (LambdaParam p (PathType Product)) (ReturnType (PathType Money))) (MemberExpr (NameExpr p) price))` |
| `do { ... } catch .a, .b { ... }` | `(DoStmt do (Block ...) (CatchClause catch (EnumCasePattern a) (EnumCasePattern b) (Block ...)))` |
| `case 0..<10` | `(MatchArm case (RangePattern (LiteralPattern 0) ..< (LiteralPattern 10)) (Block ...))` |
| `failed(let error) { ... }` | `(FailedBlock failed (BindingPattern let error) (Block ...))` |
| `use Fake for Api in .test` | `(UseDecl use (PathType Fake) for (PathType Api) in (ImplicitMemberExpr test))` |
| `use Config.sample for Config` | `(UseDecl use (MemberExpr (NameExpr Config) sample) for (PathType Config))` |
| `@keyframes fade { from { opacity: 0; } 50% { } }` | `(KeyframesRule keyframes fade (KeyframeBlock (KeyframeSelector from) (Declaration opacity (CssValue (Literal 0)))) (KeyframeBlock (KeyframeSelector 50%)))` |
| `color primary: Color = #3B82F6` | `(ThemeToken color primary (PathType Color) = (Literal #3B82F6))` |
| `variant dark { surface = #111 }` | `(ThemeVariant variant dark (ThemeOverride surface = (Literal #111)))` |
| `test "adds" { expect a == 1 }` | `(TestCaseDecl test (StringLit "adds") (Block (ExpectStmt expect (BinaryExpr (NameExpr a) == (Literal 1)))))` |
| `.a .b:hover > .c` | `(Selector (CompoundSelector (ClassSelector a)) (CompoundSelector (ClassSelector b) (PseudoClass hover)) > (CompoundSelector (ClassSelector c)))` |
| `&::before` | `(CompoundSelector (NestingSelector &) (PseudoElement before))` |
| `width: calc(100% - 2px);` | `(Declaration width (CssValue (CssFunction calc (CssValue (Literal 100%) - (Literal 2px)))))` |
| `backgroundColor: .primary.opacity(0.8);` | `(Declaration backgroundColor (CssValue (CallExpr (MemberExpr (ImplicitMemberExpr primary) opacity) (ArgList (Arg (Literal 0.8))))))` |

### 19.4 Error recovery in dumps

The parser never drops input. Tokens it cannot place are wrapped in an `ErrorNode` at the point of the
error, and missing pieces are simply absent (the diagnostic records them). The recovery rules, which the
corpus pins down case by case:

- **Junk runs.** A token that cannot start the next element of a list (an item at the top level, a member
  in a `MemberList`, a statement in a block) starts a run that is reported once (K0125, K0126 or K0122
  at its first token) and wrapped in one `ErrorNode` up to the next token that can start an element.
  Balanced `( )`, `[ ]` and `{ }` groups inside the run are skipped whole, so a misspelled `veiw { ... }`
  becomes one `ErrorNode`. A string literal, interpolations included, is skipped whole as well: a
  keyword inside `"{func f() {}}"` never starts a declaration. At the top level a stray `}` is part of
  the run; in a list it closes the list.
- **Lexer errors.** `Error` tokens (already reported as K0001) are wrapped in an `ErrorNode` where they
  occur, without a parser diagnostic, and parsing continues as if they were not there: `struct Café {}`
  dumps as `(StructDecl struct Caf (ErrorNode é) (MemberList))`, `let a: $ = 1` as
  `(FieldDecl let a (ErrorNode $) = (Literal 1))` (no K0123), and `a$` at the end of a statement does
  not start a second statement (no K0102).
- **Interpolations ended by a line break.** When a line terminator ends a string inside an
  interpolation ([§4.2](#42-interpolation-mode)), the interpolation ends there for the parser too: a
  `(`, `[` or `{` opened inside it is unclosed (K0101 at the opener) and the next line is parsed in the
  enclosing context.
- **Missing closers.** A block or member list that meets the end of file, or a token that starts a member
  or item but no statement (`style`, `func`, `init`, `rpc`, `@`, `public`, `case`, ...), ends there with
  K0101 at its `{`; the token is then parsed by the enclosing list. A parenthesized, bracketed or generic
  list that meets the end of file, a different closing delimiter or such a keyword ends with K0101 at
  its opener.
- **Missing separators.** In a delimited list, an element that starts where `,` was expected is K0100 at
  it and is parsed as the next element (`f(x y)`).
- **One diagnostic per gap.** When an enclosing construct would report "expected ..." at exactly the
  span just reported by an inner one, the second diagnostic is dropped. No two diagnostics of a parse
  share a code and a primary span.
- **Specific shapes.** `for a, b in xs` is one K0100 at the first `,` (fix: `for (a, b) in xs`), with
  the further patterns in an `ErrorNode`. `.5` and `.5em` are K0100 at the number (fix: `0.5`, `0.5em`).
  `a = b = c` is K0100 at the second operator ([§12](#12-statements)).

For example, `let = 5` dumps as
`(LetStmt let = (Literal 5))` with K0100 ("expected a name") at the `=`, and `func f() { $ }` dumps as
`(FuncDecl func f (ParamList) (Block (ErrorNode $)))` with K0001 from the lexer.

---

## 20. Token dump format

Lexer golden tests (`tests/corpus/lexer/**/*.tokens`) list one token per line as `Kind "text"`, where the
text is quoted with the escapes of [§19.1](#191-rules). `Whitespace` tokens are omitted; every other token,
including `Newline` and comments, is listed. Blank lines and lines starting with `#` are ignored.

```text
# let a = "x{y}"
LetKw "let"
Ident "a"
Eq "="
StringStart "\""
StringText "x"
InterpStart "{"
Ident "y"
InterpEnd "}"
StringEnd "\""
Newline "\n"
```

See [testing.md](testing.md) for how cases are laid out and run.
