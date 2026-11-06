# Lexer corpus

Cases for `k_syntax::lex`. Each `name.k` needs `name.tokens`; add `name.diag` when the lexer
must report diagnostics. See [docs/compiler/testing.md](../../../docs/compiler/testing.md).

Every expectation file starts with a `#` comment saying what the case checks. The rules come from
[grammar.md §2–§4](../../../docs/compiler/grammar.md#3-lexical-structure) and the lexer codes in
[diagnostics.md](../../../docs/compiler/diagnostics.md#lexer-codes).

| Directory | Covers |
|---|---|
| `trivia/` | Whitespace, the three line terminators, BOM, form feed and other non-whitespace spaces, columns after CR/CRLF |
| `comments/` | Line, doc (`///` vs `////`) and nested block comments, unterminated comments (K0003) |
| `identifiers/` | ASCII identifiers, `_`, keyword look-alikes, contextual keywords, non-ASCII characters (K0001) |
| `keywords/` | Every hard keyword, reserved words, keywords after `.` and as labels |
| `numbers/` | Integers, prefixes, floats, exponents, separators, K0006; `ranges/` and `extensions/` (`10.minutes`, `0.2.seconds`) |
| `units/` | Unit suffixes, `%` literals versus the remainder operator (RA-4) |
| `colors/` | Valid and invalid color literals (K0005) |
| `strings/` | Text, escapes, `{{`, unterminated strings and escape errors (K0002, K0004, K0007, K0008) |
| `strings/interpolation/` | Brace counting, nested strings, newlines and comments inside interpolations |
| `operators/` | Every punctuation token and maximal munch |
| `errors/` | Unexpected-character runs and recovery across several errors |
| `annotations/`, `newlines/` | Annotation token shapes; newlines are always emitted (termination is the parser's job) |
| `style/` | Entering and leaving style mode, statements, selectors, at-rules, declarations, interpolation, style-mode errors |
| `programs/` | Whole realistic files mixing every mode |
| `stress/` | Very long tokens, deep nesting and many diagnostics |
| `regressions/` | Fixed lexer bugs; see its README |
