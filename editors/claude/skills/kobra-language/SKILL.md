---
name: kobra-language
description: Core reference for the Kobra programming language (files ending in .k, projects with a k.toml, the `k` compiler). Use whenever reading, writing, reviewing or fixing Kobra code - syntax, types, optionals, errors (throws/try/catch), modules and imports, structs/enums/interfaces/classes, components, views and built-in elements, reactivity (@State, @Derived, @Effect, @Load), styles, themes and units - and to avoid TypeScript, Swift or React habits that are compile errors in Kobra.
---

# Kobra language

Kobra (`.k` files, one binary `k`) is a statically typed language that compiles to JavaScript and
targets full-stack web apps. It reads a lot like Swift, but **it isn't Swift, TypeScript or React**:
many familiar spellings are compile errors. Don't write Kobra from memory of other languages. Check
it against this skill and the compiler.

## Validate everything you write

1. **With the `kobra` MCP server** (from this plugin, `k mcp`), call `check_snippet` on every file you
   write or change before you show or save it. Pass `file_name` (such as `src/cart/CartService.k`)
   and the other files it needs in `files`. Fix every diagnostic, warnings included: `k check` fails
   on warnings.
2. For anything this skill doesn't cover, call `search_docs` (for example `@Load`, `bind`, `K3015` or
   `server streaming`), then `read_doc` on the hit (paths like `book/language/types.md`). Use
   `explain_diagnostic` for a code you don't recognise. Use `list_examples` and `read_example` to
   find idiomatic, tested code to model yours on.
3. **Without the MCP server**, run `k check` (or `k check --no-tests` for a quick type check) in the
   project and `k fmt` on the files you touched. `k check --format json` gives machine-readable
   diagnostics with their fixes, and `k fix` applies every machine-applicable fix.
4. Run `k fmt` on what you wrote, so it's in the one canonical layout (`k fmt --check` in CI).

If the compiler and this skill disagree, the compiler is right. The book (`docs/book/src`) and the
design (`docs/design`) are the sources this skill was written from.

## The rules that trip people up

- **No semicolons.** A newline ends a statement, with one statement per line. A continuation line
  starts with `.` or a binary operator (`and`, `+`). The exception is `-`: a line that starts with
  `-` is a new statement with a unary minus.
- **Logic is words:** `and`, `or`, `not`. Writing `&&`, `||` or `!` is an error (K0104 to K0106).
  `x == true` and `x == false` give the K2012 warning, so write `x` and `not x`.
- **No ternary operator.** `if` is an expression: `let label = if done { "Done" } else { "Pending" }`.
- **No force unwrap, no `try?`.** Unwrap with `if let`, `guard let`, `?.` or `??`. To turn an error into
  a value, use `try f() else { … }` or `Result.of { try f() }`.
- **Every lambda parameter is named:** `{ user => user.isActive }`, never `$0` or `it`. A lambda
  without parameters has no arrow: `{ refresh() }`. **`return` inside a lambda is an error (K2308).**
  A lambda's value is its last expression.
- **Every argument is labelled by default**, as in Swift: `greeting(name: "Ana")`. A parameter
  declared `_ id: Int` is positional, and `for product: Id` gives an external label.
- **`throws` always names one error type:** `func load() async throws CartError -> Cart`. Every throwing
  call is marked with `try`, every async call with `await`, in the order `try await`. A `catch` must be
  exhaustive.
- **`implements` is the only conformance syntax.** Write `struct P implements Codable, Equatable`,
  never `struct P: Codable`.
- **Strings interpolate with `{expr}`**, not `${}` or `\()`. A literal `{` is written `{{`, and `\{` is
  an error. Strings are single-line.
- **There are no top-level `var`s.** Shared mutable state lives in a `service`.
- **Annotations fix `let` or `var`:** `@State`, `@Binding`, `@Form` and `@Version` take `var`. `@Prop`,
  `@Derived`, `@Load`, `@Inject`, `@Param`, `@Slot`, `@Stylable`, `@Emits`, `@Env` and `@Id` take
  `let`. A mismatch is K3001.
- **Imports name exactly one declaration:** `import k.time.Clock`. There are no wildcards. Core types
  (`String`, `List`, `Money`, `Url`, `Duration`, `Color`, `Secret`, …) and the generated `Page`,
  `Endpoint`, `ThemeVariant` and `SessionUser` need no import. **The standard library's guards and
  services do:** `import k.auth.Authenticated`, `import k.auth.RateLimit`, `import k.auth.Session`,
  `import k.time.Clock`. Look a name up with `search_docs` when unsure.
- **One `public` declaration per file, named after the file.** Other declarations in the file are
  module-private helpers. A directory is a module (`src/shop/cart/` is `shop.cart`).
- **Members follow their type's visibility.** `private` is the only member modifier, and `public` on
  a member is a warning (K1302).
- **No npm, no JavaScript, no DOM.** Use the standard library: an injected `HttpClient` instead of
  `fetch`, `Clock` instead of `Date.now` or `setTimeout`, `LocalStorage`, `Json`, and so on.

## A taste

```k
import k.observe.Log

func greeting(name: String) -> String {
    return "Hello, {name}!"
}

func sizeLabel(count: Int) -> String {
    let size = match count {
        case 0 { "empty" }
        case 1..<10 { "small" }
        default { "large" }
    }
    return size
}

func describe(done: Bool, nickname: String?, names: List<String>) {
    let label = if done { "Done" } else { "Pending" }
    let displayName = nickname ?? "friend"

    if let nickname {
        Log.info("greeting {nickname}")
    }

    for name in names {
        Log.info("{label}: hello, {name} ({displayName})")
    }

    let isReady = done
        and names.isNotEmpty
    let isGuest = not isReady
}
```

```k
/// A labelled counter that reports every change.
public component Counter {
    /// What is being counted.
    @Prop let label: String
    /// Where counting starts.
    @Prop let start: Int = 0
    /// Fired with the new count after each click.
    @Emits let changed: Event<Int>

    @State var count: Int = start
    @Derived let doubled: Int = count * 2

    func increment() {
        count += 1
        changed.emit(count)
    }

    view {
        box {
            text { "{label}: {count} (× 2 = {doubled})" }
                .class(.label)

            button { "Add" }
                .on(click: increment)
        }
    }

    style {
        .label { fontSize: .lg; }
    }
}
```

Its caller passes required props as named arguments and optional ones as chained calls:
`Counter(label: "Votes").start(10).on(changed: { value => Log.info("votes: {value}") })`.

## Reference files

Load the one you need. Each file's code samples were checked with `k check`.

| File | Covers |
|---|---|
| [references/syntax.md](references/syntax.md) | `let`/`var`, functions and labels, control flow (`if`, `match`, `guard`, `for`, `while`), strings, lambdas, collections, units and money, operators, keywords |
| [references/types-and-errors.md](references/types-and-errors.md) | core types, literals and parsing, optionals, structs, enums, classes, interfaces, generics, extensions, `@Builder`, typed errors and `catch` |
| [references/components-and-views.md](references/components-and-views.md) | components, props, bindings, events, slots, reactivity (`@State`, `@Derived`, `@Effect`, `@Load`), built-in elements, attributes, focus |
| [references/styles-and-themes.md](references/styles-and-themes.md) | `style` blocks, typed classes, `when`, `@Stylable`, the theme and its tokens, variants, `Theme` |
| [references/modules-and-stdlib.md](references/modules-and-stdlib.md) | modules, imports, visibility, test-only modules, the server/client boundary, the standard library map and what replaces JavaScript APIs |
| [references/mistakes.md](references/mistakes.md) | habits from TypeScript, Swift and React that fail, with the diagnostic each one gets |

For app-level declarations (`page`, `service`, `protocol`, `entity`, `config`, jobs and so on), use the
**kobra-apps** skill. For the `k` command, tests and diagnostics, use **kobra-tooling**.

## Not built yet

The book marks some features **Planned**, and they don't compile. Among them are modifiers
(`.tooltip()`, `.animate()`), the accessible widgets in `k.ui` (`Dialog`, `Tabs`, `Menu`, …),
`k.collections`, `k.i18n`, regular expressions and Markdown in `k.text`, multi-line strings, typed npm
interop, HMR and `k doc`. `search_docs` for the feature's name, and
`read_doc book/reference/design-status.md`, show the current status.
