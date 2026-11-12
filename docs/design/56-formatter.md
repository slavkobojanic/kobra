# The formatter

## Decision

**`k fmt` has one output for every program and no options.** It works on the lossless syntax tree, keeps
every comment, and changes a program only where the language has a redundant second spelling
([01-principles.md](01-principles.md#one-obvious-way)). Line breaks are the author's: the formatter
indents, spaces and separates, but it doesn't wrap or join lines by width, apart from the few line
breaks the rules below add or remove.

The core is the `k_fmt` crate (`k_fmt::format`). The `k fmt` command, the formatting step of `k check`
and the language server's formatting request are built on it in a later round
(`(**Planned** k-fmt)` in the book until then).

```k
public component Notice {
    @Prop let message: String  // what to tell the user
    @Emits let dismissed: Event<Void>

    view {
        box {
            text { message }

            button { "Dismiss" }
                .type(.button)
                .on(click: { dismissed.emit() })
        }
        .role(.status)
        .class(.notice)
    }
}
```

formats to the same text with one space before the trailing comment, and with `box { text { message } }` on
one line its two modifiers would each go on their own line, indented one level.

## How it works

1. **The file must parse.** A file with lexer or parser errors isn't formatted; the errors are returned
   (`k_fmt::Error::Syntax`). Without a correct tree the formatter couldn't tell which newlines end a
   statement ([grammar.md §5](../compiler/grammar.md#5-newlines-and-termination)).
2. **Canonical rewrites** change tokens ([below](#canonical-rewrites)). `k_fmt::rewrite` applies only
   these.
3. **Layout** recomputes every whitespace and newline token from the tree ([below](#layout)). Nothing
   else changes.
4. **Check.** The result is parsed again and must have the tree of the rewritten text, whitespace aside
   and comments included. A difference is a formatter bug, reported as `k_fmt::Error::Internal`; the
   file is left alone.

## Layout

### Indentation

- **Four spaces per level, never tabs.** A line inside a bracket (`{ }`, `( )`, `[ ]`, a generic `< >`,
  a style value's `{ }`) is indented one level more than the line the bracket opened on, and the closing
  bracket goes back to that line's level: `foo(bar {` … `})` closes both on one line at the outer level.
- **A body is indented from its declaration or statement**, even when a clause was wrapped before its
  `{`:

  ```k
  page Admin requires Authenticated
      at "/admin" {
      view { "Admin" }
  }
  ```
- **An element starts at the bracket's level.** Elements are what a bracket holds one after another:
  statements, members, arguments, parameters, list and map entries, match arms, style items, the items of
  a file. A line that *continues* an element instead (a leading `.`, a binary operator, a wrapped
  clause, the right side after a trailing operator) is indented one level from the line of the innermost
  node that started before it: `implements A,` then `B` indents `B` from `implements`, and
  `let ok = a` then `and b` indents `and b` from `let`.
- **A modifier chain after a multi-line node stays at the node's level.** When the line before the first
  continuation line is the node's closing `}` back at the node's level, every `.modifier` line of the
  chain stays there too (`box {` … `}` then `.role(.status)`); after a one-line node they are indented
  one level (`button { "Dismiss" }` then `    .type(.button)`). These are the two layouts every
  example uses.
- **A line after a complete annotation is not a continuation:** `@Length(max: 200)` on its own line,
  then `var title: String` at the same level.
- **A comment line is indented like the code after it**; before a closing bracket, like the lines inside
  the bracket. The lines of a multi-line block comment after its first are kept as written.

### Spaces

Between two tokens on one line there is either nothing or one space:

| Where | Written |
|---|---|
| after `,`, around binary operators (`+`, `==`, `??`, `and`, …), `=`, compound assignments, `->` and `=>` | one space |
| before `,`, `:`, `?` (optional types), `)` and `]`; after `(`, `[`, `@`, `.` and `?.` | nothing |
| after `:` | one space, except in `[:]` |
| around `..<` and `...` | nothing: `0..<count`, `1...5`, as in Swift and every sample |
| a call's or declaration's `(`, an index's `[`, generic `<` `>` | attached: `max(a, b)`, `init(`, `failed(let error)`, `list[0]`, `List<Int>` |
| prefix `-` | attached: `-a`; `not` is a word and takes a space |
| `{` and `}` | a space outside and inside: `{ x => x + 1 }`, `if a {`; an empty pair is `{}` |
| a string interpolation | no spaces inside: `"{count} items"` |
| a trailing `//` comment | one space before it; the trailing comments of adjacent lines with the same indentation are aligned one space after the longest code among them |
| between words (keywords, names, literals) | one space |

**Style blocks keep the author's spaces** wherever whitespace is significant or CSS writes it freely:
`.a .b` (descendant) and `.a:hover` differ, and `padding: .sm .md` is two values. The formatter only
writes one space after a declaration's or media feature's `:` and none before it, nothing before `;` and
`,`, one space after them, and spaces around rule braces. A pair no rule covers keeps whether it had
whitespace, with one space for any amount, so the layout never joins or splits tokens it doesn't
understand.

### Line breaks

- **Kept** wherever the author wrote them, except:
- **`}` `else` and `}` `catch` join**: `} else {`, `} catch .x {` (grammar.md §5: `else` and `catch`
  continue the line before).
- **A body's `{` on its own line joins the line before** (`if ready` / `{` → `if ready {`). A lambda's
  `{` never moves: at the start of a line it starts a new statement.
- **Two or more chained modifier calls on a view node each go on their own line**
  ([05-views.md](05-views.md#formatting)). A call on a path (`Icons.check()`) is the node itself.
- Nothing joins across a comment.

### Blank lines

- **At most one blank line in a row**, none at the start or end of the file, and the file ends with one
  `\n`. Line endings are `\n` and trailing whitespace is removed, inside comments too.
- **No blank line directly after an opening bracket, before a closing one, after a doc comment or before
  a continuation line** (a chain stays attached to its node).
- **One blank line between sibling view nodes** ([05-views.md](05-views.md#formatting)) in `view`,
  `loading` and `failed` blocks and in the bodies nested in them (element and slot lambdas, `if`, `for`
  and `match` blocks). `let` constants are not view nodes: around them the author's blank lines are kept.
  A view's `metadata { … }` holds calls, not view nodes.
- **One blank line between top-level declarations**, except between imports and between top-level
  constants, where the author's blank lines are kept.
- Elsewhere (between members, between statements) the author's blank lines are kept, at most one.

## Canonical rewrites

Each is the machine-applicable fix of a redundant spelling that can be applied from the syntax tree
alone:

| Rewrite | Diagnostic | Design |
|---|---|---|
| `public` on a field, function, `init`, rpc or enum case is removed | `K1302`, `K1303` on `@Inject` | [16-modules.md](16-modules.md#visibility-of-members) |
| `private` on an `@Inject` field is removed | — (redundant, not reported) | [16-modules.md](16-modules.md#visibility-of-members) |
| a top-level, unguarded `case _ { … }` arm becomes `default { … }` | `K2303` | [08-types.md](08-types.md#match) |
| a trailing comma before `)`, `]` or `>` is removed (kept in a one-element tuple) | — | every sample |
| the last declaration of a style block gets its `;` | (`K0117` before a later one) | every sample |
| the imports at the top of the file are sorted by their text, one per line, unless a comment sits among them | — | [16-modules.md](16-modules.md#imports) |
| a service's `init()` moves after its last stored field, or, with none, before its first function or rpc, with its comments | — | [10-services.md](10-services.md#initialization-init) |

**Not rewritten:** `x == false` (`K2012`) and a `use` rule restating the default test double (`K4014`)
need types to know the rewrite is right (`x` may be a `Bool?`), so they stay with `k fix`. A guarded
`case _ where …` stays, because `default` takes no guard, and so does a `case` whose `_` has a comment
before it.

## Guarantees

- **Idempotent:** `format(format(x)) == format(x)`.
- **The same program:** the output has the tree of `rewrite(x)`, whitespace aside, comments included, and
  reports the same diagnostics as `x` apart from the ones the rewrites fix.
- **Every comment is kept**, in place, in the same order unless a moved declaration carries it.
- **Only the positions of whitespace matter:** changing the amount of whitespace, the indentation, the
  blank lines or the line endings of `x` doesn't change the output.

`crates/k_fmt/tests` checks all of these over every `.k` file in `tests/corpus`, `examples` and
`tests/fmt`, every checked `k` block of the book and programs from `k_syntax`'s structure-aware generator,
and checks the semantic diagnostics of every `sema` corpus case before and after formatting. The golden
cases are `tests/fmt/**/name.k` with `name.formatted.k` ([testing.md §5.7](../compiler/testing.md#57-formatter-properties)).

## Why

- **Keeping the author's line breaks** (as `gofmt` does) instead of wrapping at a width: a width makes
  every rename re-wrap unrelated lines, and K's newline rules make many breaks significant, so a
  wrapping formatter would need a second, width-aware parser of its own. Without it the formatter only
  ever adds or removes a line break where grammar.md §5 says it is insignificant, which the check stage
  can verify.
- **Refusing files with syntax errors:** formatting a broken file would have to guess its structure, and
  a guess that moves a newline can change what the fixed file means.
- **No alignment except trailing comments:** aligned `=` or `{` columns change whole blocks when one name
  grows. Trailing comments are aligned because a column of them is how the samples annotate
  declarations, and a run of them rarely changes.
- **Ranges without spaces:** every sample writes `0..<count`, and a range reads as one value.
- **Rewrites only from the tree:** the formatter runs without a project (one file, the language
  server's request, a pasted sample), so it never applies a fix that needs types or other files.

## Not yet

- **The `k fmt` command**, the formatting check of `k check`, and formatting in the language server
  (`textDocument/formatting`) are the next round. Until then the book's `k-fmt` notes stay.
- **Reformatting the repository.** The examples are already close to canonical (the differences are
  aligned style rules, sorted imports and blank lines between view nodes). The corpus's caret annotations
  (`//^^^ type: Int`) point at columns of the line above, so the corpus is formatted only together with
  its annotations, if ever.
- **Fuzz targets** for the formatter, on the pattern of `fuzz/` (the oracle module,
  `crates/k_fmt/tests/support/oracle.rs`, is written to be shared with one).

## Open questions

- Whether a line-width limit is ever worth its cost (see Why). Long argument lists are wrapped by hand
  today.
