// An `{error}` argument doesn't stop the generic call from typing its other parts.
func wrap<T>(value: T) -> List<T> {
    return [value]
}

let wrapped = wrap(value: nothing)
let n = wrapped.count
  //^ type: Int
