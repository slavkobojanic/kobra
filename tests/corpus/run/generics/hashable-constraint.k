// A generic function constrained to Hashable builds maps keyed by any hashable type.
func frequencies<T>(_ items: List<T>) -> Map<T, Int> where T implements Hashable {
    var counts: Map<T, Int> = [:]
    for item in items {
        counts[item] = (counts[item] ?? 0) + 1
    }
    return counts
}

func main() -> List<Int> {
    let words = frequencies(["a", "b", "a", "a"])
    let numbers = frequencies([1, 1, 2])
    return [words["a"] ?? 0, words["b"] ?? 0, numbers[1] ?? 0, numbers.count]
}
