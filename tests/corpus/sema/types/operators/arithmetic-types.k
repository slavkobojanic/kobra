// Integer arithmetic is `Int` (`/` truncates); `Double` arithmetic is `Double`.
func calc(a: Int, b: Int, x: Double, y: Double) -> Double {
    let q = a / b
      //^ type: Int
    let r = a % b
      //^ type: Int
    let z = x * y - 1.0
      //^ type: Double
    return z
}
