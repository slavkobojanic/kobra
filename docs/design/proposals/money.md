# Proposal: `Money` and `Currency`

> **Status: proposal.** Not yet accepted into the design. First used by `examples/shop`.

## Problem

`Money` is a core type ([20-standard-library.md](../20-standard-library.md)), and the design writes `9.99.usd`, `item.price * item.quantity` and `total.formatted(as: .currency)` ([05-views.md](../05-views.md), [21-testing.md](../21-testing.md)). It doesn't say how money is represented, how currencies combine, or how amounts cross to payment providers, which count in minor units (cents).

## Proposal

- **`Money` is an exact decimal amount plus a `Currency`.** It is never a floating-point number.
- **Literals are number extensions per ISO 4217 code:** `24.00.usd`, `10.eur`, `1500.jpy`. An amount with more decimal places than the currency has (`1.005.usd`) is a compile error.
- **Arithmetic:** `Money + Money` and `Money - Money` require the same currency, and `Money * Int` scales. Mixing currencies panics at runtime (it is a programmer error, like an index out of range), and is a compile error when the compiler can see both currencies. There is no `Money * Money`, and division goes through `allocated(into:)`, which splits without losing a cent.
- **Equality includes the currency:** `48.00.usd == 48.00.eur` is `false`. Comparing with `<` across currencies panics.
- **Minor units:** `money.minorUnits: Int` (`24.00.usd` → `2400`, `1500.jpy` → `1500`) and `Money(minorUnits: 2400, currency: .usd)`. These are what payment APIs use.
- **`Currency`** is an enum of ISO 4217 currencies with `code: String` (`"USD"`) and `minorUnitDigits: Int`; `Currency(code:) -> Currency?` parses an upper-case code.
- **Formatting:** `money.formatted(as: .currency)` uses the current locale ([22-metadata.md](../22-metadata.md) `locale`) and the money's own currency, so `$48.00` or `48,00 $` depending on locale.
- **Encoding:** `Codable` as `{"amount": "48.00", "currency": "USD"}`, with the amount as a decimal string so no JSON parser rounds it. In the database, `.money` columns are stored as `<name>_amount numeric(19,4)` and `<name>_currency char(3)` ([migrations.md](migrations.md)).

## Why

- Exact decimals with an attached currency rule out the two classic money bugs: float rounding and adding dollars to euros.
- Minor units are the representation every payment provider uses, so conversion is a named property rather than a `* 100` scattered through the code.
