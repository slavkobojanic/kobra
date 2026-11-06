// Money: `+` and `-` with Money, `*` by an Int quantity (docs/design/34-money.md).
let sum = 1.usd + 2.usd
  //^^^ type: Money
let less = 3.usd - 1.usd
  //^^^^ type: Money
let scaled = 2.50.usd * 3
  //^^^^^^ type: Money
