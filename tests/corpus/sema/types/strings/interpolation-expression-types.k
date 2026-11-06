// Interpolated expressions are typed like any expression.
func label(a: Int, b: Int, name: String) -> String {
    return "sum {a + b} of {name.uppercased()}"
               //^^^^^ type: Int
                          //^^^^^^^^^^^^^^^^^ type: String
}
