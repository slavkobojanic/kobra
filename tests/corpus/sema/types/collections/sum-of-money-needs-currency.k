// A sum over `Money` needs a currency (docs/design/34-money.md): `sum()` is K2402 with a fix to
// `sum(in:_:)`, which stays valid next to it.
func totals(prices: List<Money>) -> Money {
    let wrong = prices.sum()
    let right = prices.sum(in: .usd) { price => price }
      //^^^^^ type: Money
    return right
}
