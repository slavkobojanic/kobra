// `Money + Money` and `Money - Money` need one currency (34-money.md): mixing two at run time panics,
// so mixing two literal currencies the checker can see is K2407 at the whole expression, through
// parentheses, negation, scaling and the sums inside. The result is still `Money`, so nothing
// cascades. Amounts whose currency only the run time knows are not checked.
let mixed = 1.usd + 2.eur
  //^^^^^ type: Money
let less = 3.gbp - 1.usd
let grouped = (1.usd + 2.usd) - (-3.eur)
let scaled = 2.usd * 3 + 1.jpy
let nested = 1.usd + 2.usd + 3.chf
let same = (1.usd + 2.usd) - 3.usd * 2

func total(a: Money) -> Money {
    return a + 1.eur
}
