// An interpolated string is a `String`, whatever the interpolated types.
func label(count: Int, price: Double, name: String, done: Bool) -> String {
    let text = "{name}: {count} at {price}, done {done}"
      //^^^^ type: String
               //^^^^ type: String
                       //^^^^^ type: Int
    return text
}
