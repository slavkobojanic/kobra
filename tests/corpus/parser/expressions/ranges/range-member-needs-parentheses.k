// postfix binds tighter: `(0..<10).contains(x)` needs parentheses, `0..<10.count` ranges to `10.count`
let a = (0..<10).contains(x)
let b = 0..<10.count
