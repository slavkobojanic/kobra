// No `/` on Money, no `Money * Money`, and a rate goes through
// `multiplied(by:rounding:)`, not `*` (docs/design/34-money.md): K2013.
let half = 1.usd / 2
let product = 1.usd * 2.usd
let taxed = 10.usd * 1.08
