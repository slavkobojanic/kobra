// A lambda with more parameters than the expected function type is K2001 at the lambda.
func doubled(numbers: List<Int>) -> List<Int> {
    return numbers.map { a, b => a + b }
}
