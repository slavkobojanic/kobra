// A type error inside an interpolation is reported at the inner expression.
func label(count: Int, name: String) -> String {
    return "{count - name}"
}
