// `keys` and `values` are in insertion order; overwriting a key keeps its position.
func main() -> List<List<String>> {
    var m: Map<String, Int> = [:]
    m["b"] = 1
    m["a"] = 2
    m["c"] = 3
    m["b"] = 4
    return [m.keys, m.values.map { v => "{v}" }]
}
