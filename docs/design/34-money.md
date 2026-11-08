# Money

## Decision

**`Money` is an exact decimal amount plus a `Currency`.** It is never a floating-point number, and it never silently mixes currencies or rounds.

```k
let unitPrice = 24.00.usd
let total = unitPrice * quantity                                      // Money * Int
let withTax = total.multiplied(by: 1.0825, rounding: .halfEven)        // an explicit rounding rule
let cartTotal = items.sum(in: .usd) { item => item.price * item.quantity }

text { total.formatted(as: .currency) }                               // "$48.00", by the page's locale
```

## Rules

- **Literals are number extensions per ISO 4217 code:** `24.00.usd`, `10.eur`, `1500.jpy`. An amount with more decimal places than the currency has (`1.005.usd`, `1.5.jpy`) is a compile error (`K2404`). Trailing zeros don't count, as they don't change the amount: `1.500.usd` is `1.50.usd`, the same rule the runtime applies to amounts it parses.
- **An amount always holds exactly its currency's minor-unit digits.** Only `multiplied(by:rounding:)` can produce more, and it rounds back before returning.
- **Arithmetic:**
  - `Money + Money` and `Money - Money` require the same currency. Mixing currencies panics at runtime (a programmer error, like an index out of range), and is a compile error when the compiler can see both currencies.
  - `Money * Int` scales by a quantity.
  - **`money.multiplied(by: Decimal, rounding: RoundingRule) -> Money`** scales by a rate (tax, discount). The rounding rule is required (`.halfEven`, `.halfUp`, `.down`, `.up`), so rounding is always a visible decision.
  - There is no `Money * Money` and no `/`. Division goes through `allocated(into:)`, which splits without losing a cent.
- **Sums need a currency.** `sum` over `Money` without one is a compile error, with a quick fix to **`sum(in: Currency)`**, which returns `Money.zero(currency)` for an empty list. An empty cart then has a total.
- **Equality includes the currency:** `48.00.usd == 48.00.eur` is `false`. Comparing with `<` across currencies panics.
- **Minor units:** `money.minorUnits: Int` (`24.00.usd` → `2400`, `1500.jpy` → `1500`) and `Money(minorUnits: 2400, currency: .usd)`. These are what payment APIs use.
- **`Currency`** (core) is an enum of ISO 4217 currencies with `code: String` (`"USD"`) and `minorUnitDigits: Int`. `Currency.parse(code:) -> Currency?` parses an upper-case code ([20-standard-library.md](20-standard-library.md#literals-and-parsing)).
- **Formatting:** `money.formatted(as: .currency)` uses the locale of the app's or page's `metadata`, on the server, in the browser and in background jobs alike ([22-metadata.md](22-metadata.md)), and the money's own currency: `$48.00` or `48,00 $`.

## Encoding

- **JSON:** `Codable` as `{"amount": "48.00", "currency": "USD"}`, with the amount as a decimal string so no JSON parser rounds it.
- **Database:** a `.money` column is one logical column stored as two ([19-persistence.md](19-persistence.md#the-schema-builder)): `<name>_amount` and `<name>_currency`. On Postgres the amount is `numeric(19,4)`. **On SQLite it is `TEXT`,** because SQLite's numeric affinity would store a decimal as a floating-point `REAL`, and development and tests would round, which is the bug `Money` exists to prevent. The currency is `char(3)` on Postgres and `TEXT` on SQLite.

## Why

- Exact decimals with an attached currency rule out the two classic money bugs: float rounding and adding dollars to euros.
- Requiring a rounding rule and a currency for sums turns the remaining silent decisions into explicit ones.
- Minor units are the representation every payment provider uses, so conversion is a named property rather than a `* 100` scattered through the code.
- `Decimal`, `Money` and `Currency` ship to the browser as a chunk of their own, only to apps whose client code uses them, so exact arithmetic costs nothing in apps that never show a price ([02-compilation.md](02-compilation.md#consequences)).
