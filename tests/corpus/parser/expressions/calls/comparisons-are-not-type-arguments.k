// `<` and `>` stay comparisons unless written as type arguments: spaced, lowercase or across lines
let a = f(x < y, z > (w))
let b = f(x<y, z>(w))
let c = f(A < B, C > (w))
let d = count<limit
