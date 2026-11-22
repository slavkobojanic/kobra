# Units and number extensions

Web code is full of quantities: pixel widths, animation durations, cache lifetimes, upload limits. In
most languages they're plain numbers, and the unit lives in a variable name or a comment. In Kobra, **units
are part of the type system**.

## Unit literals

A number followed directly by a CSS unit is a unit literal:

```k,fails=K2101
let delay: Duration = 200ms
let width = 100px + 2rem          // Length; compiles to calc(100px + 2rem)
let gutter = 16px * 2             // folded to 32px at compile time
let bad = 100px + 200ms           // compile error: Length + Duration
```

| Literal | Type |
|---|---|
| `16px`, `1.5rem`, `2em`, `1ch`, `50vh`, `100vw`, `1fr` | `Length` (or a subtype) |
| `50%` | `Percent` |
| `200ms`, `2s` | `Duration` |
| `45deg` | `Angle` |
| `#3B82F6` | `Color` |

The full set of length units is `px`, `em`, `rem`, `ch`, `vw`, `vh`, `vmin`, `vmax`, `dvh`, `svh`,
`lvh` and `fr`. An unknown suffix is a compile error.

Units are **native literals in the core language**, not only in styles. A `Duration` is a `Duration`
whether you use it in a transition, a timer or a cache.

## Unit arithmetic

- Lengths with the same unit are **folded at compile time**: `16px * 2` is `32px`.
- Lengths with different units become CSS `calc()`: `100px + 2rem`.
- Adding quantities of different kinds, such as a length and a duration, is a compile error.
- **Lengths can't be ordered.** `<`, `>`, `<=` and `>=` on two `Length`s are a compile error (`K2013`),
  even with one unit, because the answer to `100px > 2rem` depends on the root font size. `==` works.

```k,fails=K2013
let isWide = 100px > 2rem          // compile error: `Length` and `Length` can't be ordered
```

Because `%` right after a number makes a `Percent` literal, the remainder operator on literals needs
spaces: write `10 % 3`, not `10%3`.

## Number extensions

Units that have no CSS suffix are **extension properties on numbers**, in the style of Rails:

```k
~import k.time.Clock
~
~service Presence {
~    @Inject let clock: Clock
~
~    func refresh() {}
~
~    func start() {
let timeout = 10.minutes
let expiresAt = clock.now + 3.days        // clock: an injected Clock
let lastSeen = clock.now - 2.hours
let maxUpload = 25.megabytes

clock.every(30.seconds) { refresh() }
~    }
~}
```

- **CSS units are literal suffixes** (`px`, `rem`, `em`, `%`, `vh`, `vw`, `fr`, `deg`, `ms`, `s`).
  **Everything else is an extension property:** `.seconds`, `.minutes`, `.hours`, `.days`, `.weeks`,
  `.bytes`, `.kilobytes`, `.megabytes`, and so on.
- Singular forms are aliases: `1.hour`, `1.day`.
- Both forms produce the same types: `200ms == 0.2.seconds` is `true`.

**Durations are plain values; the current time is not.** `3.days` needs no clock, but "now" comes only
from an injected `Clock` service, which tests replace with a virtual clock. There is no `Date.now`,
`.ago` or `.fromNow`: "two hours ago" is `clock.now - 2.hours`. See [Time and timers](../apps/time.md).

## Money

Money uses the same idea, with one extension per currency code: `24.00.usd`, `10.eur`, `1500.jpy`.
`Money` is an exact decimal amount plus a currency, never a floating-point number, and it never
silently mixes currencies or rounds. It has its own chapter: [Money](../apps/money.md).

## Defining your own

Number extensions are an ordinary language feature. A library or an app can add its own with
[`extend`](declarations.md#extensions):

```k
~struct RetryPolicy {
~    let attempts: Int
~}
~
extend Int {
    var retries: RetryPolicy { RetryPolicy(attempts: self) }
}

let policy = 3.retries
```
