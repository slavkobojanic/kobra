// The result type of a generic call is the instantiated return type.
func wrap<T>(value: T) -> List<T> {
    return [value]
}

let wrapped = wrap(value: 2.5)
  //^^^^^^^ type: List<Double>
