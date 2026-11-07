// Int `+ - *` are checked in development and test builds, `/` and `%` always
// (docs/compiler/codegen.md §5); Double arithmetic is plain.
func mix(a: Int, b: Int, x: Double) -> Double {
    let sum = a + b
    let quotient = a / b
    let rest = a % b
    return x * 2.0
}
