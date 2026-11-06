// `not a == b` is `not (a == b)`; `a ?? 0 > 1` is `(a ?? 0) > 1`: both are well-typed.
func check(a: Int, b: Int, c: Int?) -> Bool {
    let x = not a == b
      //^ type: Bool
    let y = c ?? 0 > 1
      //^ type: Bool
    return x and y
}
