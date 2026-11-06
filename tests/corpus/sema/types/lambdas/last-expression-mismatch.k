// Against an expected `(String) -> Int`, a lambda whose last expression is a `String` is K2001 at that expression.
func lengths(words: List<String>) -> List<Int> {
    let mapper: (String) -> Int = { word => word.trimmed() }
    return words.map(mapper)
}
