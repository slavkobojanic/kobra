// `{ (a) }` and `{ (a, b) }` have no `=>`, so the parentheses are the body (bounded lookahead)
let a = { (x) }
let b = { (x, y) }
let c = { (x) + 1 }
