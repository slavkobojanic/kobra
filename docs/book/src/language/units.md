# Units and number extensions

Web code is full of quantities: pixel widths, animation durations, cache lifetimes, upload limits. In
most languages they're plain numbers, and the unit lives in a variable name or a comment. In K, **units
are part of the type system**.

## Unit literals

A number followed directly by a CSS unit is a unit literal:

```k
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
- **Comparing lengths with different units** (`100px > 2rem`) is a compile error outside a layout
  context, because the answer depends on the root font size.

Because `%` right after a number makes a `Percent` literal, the remainder operator on literals needs
spaces: write `10 % 3`, not `10%3`.

## Number extensions

Units that have no CSS suffix are **extension properties on numbers**, in the style of Rails:

```k
let timeout = 10.minutes
let expiresAt = clock.now + 3.days        // clock: an injected Clock
let lastSeen = clock.now - 2.hours
let maxUpload = 25.megabytes

Cache.set(key, value, ttl: 1.hour)
clock.every(30.seconds) { refresh() }
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

Money uses the same idea, with one extension per ISO 4217 currency code:

```k
let unitPrice = 24.00.usd
let total = unitPrice * quantity                                      // Money * Int
let withTax = total.multiplied(by: 1.0825, rounding: .halfEven)        // an explicit rounding rule
let cartTotal = items.sum(in: .usd) { item => item.price * item.quantity }
let yen = 1500.jpy

text { total.formatted(as: .currency) }                               // "$48.00", by the page's locale
```

`Money` is an exact decimal amount plus a `Currency`, never a floating-point number, and it never silently
mixes currencies or rounds:

- **An amount always holds exactly its currency's minor-unit digits.** A literal with more decimal places
  than the currency has, such as `1.005.usd`, is a compile error.
- **`Money + Money` and `Money - Money` need the same currency.** Mixing currencies is a compile error
  when the compiler can see both, and a panic otherwise. `48.00.usd == 48.00.eur` is `false`.
- **`Money * Int` scales by a quantity.** Scaling by a rate goes through
  `multiplied(by: Decimal, rounding:)`, whose rounding rule (`.halfEven`, `.halfUp`, `.down`, `.up`) is
  required. There is no `Money * Money` and no `/`; `allocated(into:)` splits an amount without losing a
  cent.
- **Sums need a currency.** `sum(in: .usd) { … }` returns `Money.zero(.usd)` for an empty list, so an empty
  cart still has a total. `sum` over money without a currency is a compile error with a quick fix.
- **Minor units** are what payment APIs use: `24.00.usd.minorUnits` is `2400`, and
  `Money(minorUnits: 2400, currency: .usd)` goes back. `Currency.parse(code: "USD")` parses a code.
- **Formatting** uses the locale of the app's or page's `metadata`, on the server, in the browser and in
  background jobs alike, never the browser's language setting.

In JSON, money is `{"amount": "48.00", "currency": "USD"}`, with the amount as a string so no JSON parser
rounds it. In the database a `Money` field is one logical column stored as two, `<name>_amount` and
`<name>_currency`.

## Defining your own

Number extensions are an ordinary language feature. A library or an app can add its own with
[`extend`](declarations.md#extensions):

```k
extend Int {
    var retries: RetryPolicy { RetryPolicy(attempts: self) }
}

let policy = 3.retries
```
