# Operators and precedence

> This page summarizes the operators. The authoritative definition is the compiler grammar,
> `docs/compiler/grammar.md` in the repository (section 13.2, "Precedence and associativity"). This page
> will link to the published grammar once it is part of the site; if the two ever disagree, the grammar
> wins.

## Operators

| Operator | Meaning |
|---|---|
| `+` `-` `*` `/` `%` | arithmetic; `%` is the remainder |
| `-x` | negation |
| `==` `!=` | equality |
| `<` `<=` `>` `>=` | ordering |
| `and` `or` `not` | logic |
| `??` | default for an optional: `nickname ?? "friend"` |
| `?.` | optional chaining: `user?.address` |
| `a..<b`, `a...b` | half-open and closed ranges |
| `=` `+=` `-=` `*=` `/=` `%=` | assignment (statements, never expressions) |
| `.name` | member access, or an implicit member when there is no receiver |
| `=>` | starts a lambda's body |
| `->` | introduces a return type |
| `@` | starts an annotation |

`bind x` and `x when condition` are only allowed as argument values (and `when` also as a list element),
so they don't appear in the precedence table.

## Precedence

From lowest to highest:

| Level | Operators | Associativity |
|---|---|---|
| 1 | `try`, `await` (prefix) | right; cover the whole expression to their right |
| 2 | `or` | left |
| 3 | `and` | left |
| 4 | `not` (prefix) | right |
| 5 | `==` `!=` `<` `<=` `>` `>=` | none |
| 6 | `??` | right |
| 7 | `..<` `...` | none |
| 8 | `+` `-` | left |
| 9 | `*` `/` `%` | left |
| 10 | `-` (prefix) | right |
| 11 | calls, trailing lambdas, `.` and `?.` member access, `[ ]` indexing | left |

Consequences worth knowing:

- Comparisons and ranges don't chain: `a < b < c` is a compile error.
- `not a == b` means `not (a == b)`; `not a and b` means `(not a) and b`.
- `-5.minutes` means `-(5.minutes)`.
- `a ?? b ?? c` means `a ?? (b ?? c)`.
- `try` and `await` must start the expression they cover: write `let x = try f() + 1`, not
  `let x = 1 + try f()`. Inside arguments and parentheses they are fine: `f(try g())`.

## What K doesn't have

| Not in K | Use instead |
|---|---|
| `a ? b : c` | `if a { b } else { c }` |
| `&&`, `\|\|`, `!` | `and`, `or`, `not` |
| `try?`, `!` (force unwrap) | `try … else { … }`, `Result.of { … }`, `if let`, `guard let` |
| `$0`, `it` | named lambda parameters |
| `;` between statements | a new line |
| `tuple.0` | a pattern, or a struct with named fields |
| `x == true`, `x == false` | `x`, `not x` (the comparison is reported with a fix) |
| `f<Int>(x)` | an annotated binding: `let tags: Set<String> = Set([])` |
