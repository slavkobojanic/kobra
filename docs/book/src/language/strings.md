# Strings and interpolation

## String literals

Strings are written in double quotes and may contain any Unicode text:

```k
let greeting = "Hello, world"
let farewell = "Ha det bra 👋"
```

**Strings are single-line.** A string can't contain a raw line break; use `\n` for one. Multi-line string
literals aren't part of the language yet.

## Interpolation

Inside a string, `{ }` holds a K expression, and its value is inserted into the text:

```k
let name = "Ana"
let items = 3

let message = "{name} has {items} items in the cart"
let total = "Total: {price * quantity}"
let status = "You are {if isSignedIn { "signed in" } else { "a guest" }}"
```

This is the same `{ }` that means "a K expression goes here" everywhere in K, including in
[styles](../apps/styles.md). Any expression works, including calls, member access and `if`
expressions, as long as it stays on one line.

To write a literal brace, double it: `"{{"` is one `{`. A closing `}` needs no escape:

```k
let template = "Use {{name}} as a placeholder"     // Use {name} as a placeholder
```

## Escapes

| Escape | Character |
|---|---|
| `\n` | line feed |
| `\r` | carriage return |
| `\t` | tab |
| `\0` | NUL |
| `\\` | backslash |
| `\"` | double quote |
| `\u{1F44B}` | the Unicode scalar value with that hexadecimal code |
| `{{` | `{` |

Any other backslash sequence is a compile error. In particular, `\{` is an error with a quick fix to
`{{`.

## Strings in views

Inside a view, a string literal *is* a text node. No wrapper is needed:

```k
view {
    paragraph { "Welcome back, {user.name}." }

    button { "Sign out" }
        .on(click: signOut)
}
```

Interpolated values are always inserted as text, never as HTML, so user input can't inject markup.

## Formatting values

Values that have more than one sensible text form are formatted explicitly with `formatted(as:)`:

```k
let price = 48.00.usd

paragraph { "Total: {price.formatted(as: .currency)}" }    // Total: $48.00, depending on the locale
```

Locale-aware number, date and currency formatting lives in `k.i18n` and `k.time`; text utilities such as
regular expressions, slugs and Markdown rendering live in `k.text`. See the
[standard library overview](../reference/stdlib.md).

## Secrets in strings

> **Proposal.** `Secret<T>` follows `docs/design/proposals/config-and-secrets.md`.

A configuration value of type `Secret<String>` interpolates as `Secret(•••)`, never as its value, so
logging a configuration can't leak it. Code that really needs the value calls `.reveal()`, which is easy
to search for in a review.
