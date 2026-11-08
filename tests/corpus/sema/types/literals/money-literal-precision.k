// A money literal can't have more decimal places than its currency (34-money.md): `1.005.usd` is
// K2404. Trailing zeros don't count, as the amount is the same.
let price = 24.00.usd
let trailing = 1.500.usd
let yen = 1500.jpy
let wholeYen = 1500.0.jpy

let tooFine = 1.005.usd
let fractionalYen = 1.5.jpy
