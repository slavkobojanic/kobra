// Maps have value semantics like lists.
func main() -> List<Int> {
    let original: Map<String, Int> = ["a": 1]
    var copy = original
    copy["b"] = 2
    copy["a"] = 10
    return [original.count, original["a"] ?? 0, copy.count, copy["a"] ?? 0]
}
