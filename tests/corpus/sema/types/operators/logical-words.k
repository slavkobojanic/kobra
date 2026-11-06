// `and`, `or` and `not` take and give `Bool`.
func check(a: Bool, b: Bool) -> Bool {
    let both = a and b
      //^^^^ type: Bool
    let either = a or not b
      //^^^^^^ type: Bool
    return not both
}
