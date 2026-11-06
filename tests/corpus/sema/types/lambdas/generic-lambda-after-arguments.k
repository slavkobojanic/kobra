// Generic arguments are fixed by non-lambda arguments first, then the lambda is checked against them.
func apply<T, U>(value: T, transform: (T) -> U) -> U {
    return transform(value)
}

let doubled = apply(value: 21) { n => n * 2 }
  //^^^^^^^ type: Int
                               //^ type: Int
let text = apply(value: 1.5) { x => "{x}" }
  //^^^^ type: String
