// `T` converts implicitly to `T?` in bindings, arguments and returns.
func wrap(n: Int) -> Int? {
    let maybe: Int? = n
                    //^ type: Int
    return n
}

let x = wrap(n: 3)
  //^ type: Int?
