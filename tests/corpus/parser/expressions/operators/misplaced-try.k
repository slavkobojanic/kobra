// `try` after a binary operator is K0114; the tree still builds `a + (try b)`
let a = b + try c()
