// The value of a multi-statement lambda is its last expression.
func lengths(words: List<String>) -> List<Int> {
    return words.map { word =>
        let trimmed = word.trimmed()
        trimmed.count
    }
}
//@ 3:12-6:6 type: List<Int>
