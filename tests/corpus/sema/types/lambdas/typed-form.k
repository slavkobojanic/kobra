// The typed form `{ (a: A, b: B) -> R => ... }` has its written function type without context.
let add = { (a: Int, b: Int) -> Int => a + b }
  //^^^ type: (Int, Int) -> Int
let sum = add(1, 2)
  //^^^ type: Int
