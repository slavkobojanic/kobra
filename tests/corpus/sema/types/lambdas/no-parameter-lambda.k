// A lambda without parameters has type `() -> T` from its last expression.
let answer = { 42 }
  //^^^^^^ type: () -> Int
let value = answer()
  //^^^^^ type: Int
