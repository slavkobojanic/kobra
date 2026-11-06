// A parameter of function type is callable; its result has the declared type.
func apply(transform: (Int) -> String, value: Int) -> String {
    return transform(value)
         //^^^^^^^^^^^^^^^^ type: String
}
