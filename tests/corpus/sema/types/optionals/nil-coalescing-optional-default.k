// With an optional right side, `??` stays optional.
func pick(a: Int?, b: Int?) -> Int? {
    let value = a ?? b
      //^^^^^ type: Int?
    return value
}
