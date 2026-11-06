// `??` is right-associative: `a ?? b ?? c` with a non-optional `c` is non-optional.
func pick(a: Int?, b: Int?) -> Int {
    let value = a ?? b ?? 0
      //^^^^^ type: Int
              //^^^^^^^^^^^ type: Int
    return value
}
