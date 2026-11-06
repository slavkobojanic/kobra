# Parser corpus: declarations

Golden cases for every declaration kind and the parts they share: prefixes (annotations, `public`,
`private`, `static`), clauses (`requires`, `at`, `implements`, `for`, `where`, `throws`, `->`), generic
parameters, parameter lists, imports and `use` rules. Formats are described in
[docs/compiler/testing.md](../../../../docs/compiler/testing.md); the grammar is
[docs/compiler/grammar.md](../../../../docs/compiler/grammar.md) §7–§10 and §17.

Every `.k` file starts with a `//` comment saying what it checks (so diagnostics are on line 2 or later),
except `files/empty.k` and the BOM case, where the comment would change the behavior under test.

| Directory | Covers |
|---|---|
| `files/` | empty files, comment-only files, BOM, CRLF and lone CR, missing final newline, blank lines, every item kind in one file, doc comments as trivia |
| `imports/` | dotted, host/slash/hyphen and keyword paths, `import proto`, comments, K0111, truncated imports |
| `visibility/` | `public`/`private`/`static` on items and members, annotation order (K0121), duplicate modifiers (K0127) |
| `annotations/` | bare, labeled, implicit-member, string, range, empty, multi-line and nested arguments; placement on every declaration and on parameters |
| `component/`, `page/`, `layout/`, `service/`, `protocol/`, `entity/`, `repository/` | the UI, service and persistence declarations with all their clauses and members |
| `struct/`, `enum/`, `class/`, `interface/`, `extend/` | type declarations, generics, `where`, cases with associated values, initializers, computed properties, default methods |
| `theme/`, `app/`, `tests/` | theme tokens and variants, `App.k` (`use … for … in …`, `metadata`), test suites and cases |
| `func/`, `params/` | signatures (`async`, `throws E`, `-> T`, `where`), labels, defaults, function-typed parameters |
| `formatting/` | comments in every header position, Allman braces, wrapped clauses, extra spaces, compact bodies |
| `unicode/` | Unicode strings and comments, scalar-value columns, non-ASCII identifiers |
| `errors/` | recovery: junk items and members, several errors in one file, semicolons, unclosed bodies |
| `large/` | generated stress cases: hundreds of members, items, imports and parameters; 4096-character names; deep nesting |

The `large/` cases are generated; keep them in sync by regenerating rather than editing by hand.

Two conventions these cases rely on that grammar.md states only through its examples:

- The `<` and `>` of `GenericParamList` and `GenericArgList` are omitted from dumps, like other brackets.
- A string literal without text (`""`) dumps as `(StringLit)`.
