// A `guard let` binding stays unwrapped for the rest of the function.
func length(text: String?) -> Int {
    guard let value = text else { return 0 }
    return value.count
         //^^^^^ type: String
}
