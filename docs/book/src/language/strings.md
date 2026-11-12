# Strings and interpolation

## String literals

Strings are written in double quotes and may contain any Unicode text:

```k
let greeting = "Hello, world"
let farewell = "Ha det bra 👋"
```

**Strings are single-line.** A string can't contain a raw line break; use `\n` for one. An
interpolation inside a string can't span lines either. Multi-line string literals aren't part of the
language yet.

## Interpolation

Inside a string, `{ }` holds a K expression, and its value is inserted into the text:

```k
~let price = 12.00.usd
~let quantity = 2
~let isSignedIn = true
~
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
let template = "Use {{name}} as a placeholder" // Use {name} as a placeholder
let broken = "{{ this is not JSON"             // { this is not JSON
```

Because `{` always starts an interpolation, a string such as `"{ this is not JSON"` doesn't mean what it
seems to: it tries to interpolate an expression. Write `{{` whenever you mean a brace.

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

Any other backslash sequence is a compile error. In particular, `\{` is an error (`K0007`) with a quick
fix to `{{`.

## Strings in views

Inside a view, a string literal *is* a text node. No wrapper is needed:

```k
~struct User {
~    let name: String
~}
~
~component Greeting {
~    @Prop let user: User
~
~    func signOut() {}
~
view {
    paragraph { "Welcome back, {user.name}." }

    button { "Sign out" }
        .on(click: signOut)
}
~}
```

Interpolated values are always inserted as text, never as HTML, so user input can't inject markup.

## Formatting values

Values that have more than one sensible text form are formatted explicitly with `formatted(as:)`:

```k
~component Total {
let price = 48.00.usd

~view {
paragraph { "Total: {price.formatted(as: .currency)}" }    // Total: $48.00, depending on the locale
~}
~}
```

`formatted(as:)` is available on numbers, [money](../apps/money.md) and [dates](../apps/time.md).

> **Planned** (k-text, k-i18n). The design gives `k.text` regular expressions, slugs and Markdown rendering, and adds a
> `k.i18n` module for localized strings and plurals. Neither is built yet: `k.text` holds only
> `EmailAddress` today. See the [standard library overview](../reference/stdlib.md).

## Secrets in strings

A value of type `Secret<String>`, such as an API key read from [configuration](../apps/config.md),
interpolates as `Secret(•••)`, never as its value, so logging a configuration can't leak it:

```k
~import k.observe.Log
~
~config WebhookConfig {
~    @Env("WEBHOOK_SECRET") let signingSecret: Secret<String>
~}
~
~func verify(config: WebhookConfig) {
Log.info("verifying with {config.signingSecret}")      // verifying with Secret(•••)
~}
```

Most code never needs the value: outgoing HTTP requests and `Hmac` take secrets directly. Code that
really does calls `.reveal()`, which is easy to search for in a review. Comparing a secret, or an
expression that calls `.reveal()` (directly or inside an interpolation), with `==` or `!=` is a compile
error (`K2001`) that points to the constant-time `secret.matches(candidate)`:

```k,fails=K2001
~func isAuthorized(token: Secret<String>, header: String) -> Bool {
return header == "Bearer {token.reveal()}"      // compile error: compare with `token.matches(…)`
~}
```

The check reads the comparison itself, so a revealed value stored in a local first isn't caught: compare
with `matches` rather than revealing.
