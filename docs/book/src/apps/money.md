# Money

**`Money` is an exact decimal amount plus a `Currency`.** It is never a floating-point number, and it
never silently mixes currencies or rounds. It is a core type, so it needs no import.

<!-- The declarations the samples in this chapter use; checked, not rendered.

```k
// src/store/Context.k
struct CartItem {
    let price: Money
    let quantity: Int
}
```
-->

```k
~component CartTotal {
~    @Prop let items: List<CartItem>
~    @Prop let quantity: Int
~
let unitPrice = 24.00.usd
let total = unitPrice * quantity                                      // Money * Int
let withTax = total.multiplied(by: 1.0825, rounding: .halfEven)        // an explicit rounding rule
let cartTotal = items.sum(in: .usd) { item => item.price * item.quantity }
let yen = 1500.jpy

~    view {
text { total.formatted(as: .currency) }                               // "$48.00", by the page's locale
~    }
~}
```

## Literals

A money literal is a number followed by a currency extension: `24.00.usd`, `10.eur`, `1500.jpy`.

- **An amount always holds exactly its currency's minor-unit digits.** A literal with more decimal
  places than the currency has, such as `1.005.usd`, is a compile error.
- `Money(minorUnits: 2400, currency: .usd)` builds the same amount as `24.00.usd` from minor units, the
  representation payment APIs use, and `24.00.usd.minorUnits` is `2400`.
- `Currency.parse(code: "USD")` parses an upper-case ISO 4217 code and returns `Currency?`;
  `currency.code` is `"USD"` and `currency.minorUnitDigits` is `2` (`0` for the yen).

> **Planned.** The design makes every ISO 4217 code a `Currency` case and a literal extension. Five are
> built today: `usd`, `eur`, `gbp`, `jpy` and `chf`.

## Arithmetic

- **`Money + Money` and `Money - Money` need the same currency.** Mixing currencies is a compile error
  when the compiler can see both, and a panic otherwise, like an index out of range.
- **`Money * Int` scales by a quantity.**
- **Scaling by a rate goes through `multiplied(by:rounding:)`**, and the rounding rule is required:
  `.halfEven`, `.halfUp`, `.down` or `.up`. Rounding is always a visible decision.
- **There is no `Money * Money` and no `/`.** `allocated(into:)` splits an amount into parts that add up
  to it exactly, differing by at most one minor unit:

```k
~func split() -> List<Money> {
let shares = 10.00.usd.allocated(into: 3)          // [3.34.usd, 3.33.usd, 3.33.usd]
~    return shares
~}
```

- **Equality includes the currency:** `48.00.usd == 48.00.eur` is `false`. Comparing with `<` across
  currencies panics.

## Sums need a currency

A sum over money names its currency, so an empty list still has a total, `Money.zero(.usd)`:

```k
~func total(items: List<CartItem>) -> Money {
~    return
items.sum(in: .usd) { item => item.price * item.quantity }
~}
```

`sum()` over money without a currency is `K2402`, with a quick fix:

```k,fails=K2402
~func total(prices: List<Money>) -> Money {
~    return
prices.sum()                                        // error: a sum over `Money` needs a currency
~}
```

## Formatting

`money.formatted(as: .currency)` uses the money's own currency and the locale of the app's or page's
`metadata` ([Document metadata](metadata.md)), on the server, in the browser and in background jobs
alike, never the browser's language setting: `$48.00`, or `48,00 $`.

## On the wire and in the database

- **JSON:** `{"amount": "48.00", "currency": "USD"}`, with the amount as a decimal string so no JSON
  parser rounds it. RPC messages use the same shape.
- **Database:** a `Money` field is one logical column stored as two, `<name>_amount` and
  `<name>_currency`. On Postgres the amount is `numeric(19,4)`; on SQLite it is `TEXT`, because SQLite's
  numeric affinity would store a decimal as a floating-point number. In the schema builder the column
  type is `.money` ([Persistence](persistence.md#the-schema-builder)).

The shop example keeps prices and order totals as `Money` from the entity to the confirmation email:

```k
// examples/shop/shop/src/store/StoreApi.k
total: product.price * request.quantity,
```

```k
// examples/shop/shop/src/store/OrderConfirmationJob.k
"Total paid: {order.total.formatted(as: .currency)}",
```
