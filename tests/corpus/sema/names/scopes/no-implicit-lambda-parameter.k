// K has no implicit lambda parameter: `it` is an ordinary name, and unresolved here.
func doubled(values: List<Int>) -> List<Int> {
    return values.map { it * 2 }
}
