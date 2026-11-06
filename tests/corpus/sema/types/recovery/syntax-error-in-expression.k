// A missing operand is a parser error (K0122); the checker adds no type error for the missing part.
let x: Int = (1 + )
let y: Int = x
  //^ type: Int
