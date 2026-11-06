// `return` in the function itself, after a lambda, is fine.
func doubled(values: List<Int>) -> List<Int> {
    let result = values.map { value => value * 2 }
    return result
}
