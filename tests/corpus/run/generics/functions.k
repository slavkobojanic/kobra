// Generic functions are erased; `Comparable` constraints work on any comparable type.
func largest<T>(items: List<T>) -> T? where T implements Comparable {
    var best: T? = nil
    for item in items {
        if let current = best {
            if item > current {
                best = item
            }
        } else {
            best = item
        }
    }
    return best
}

func main() -> List<String> {
    let none: List<Int> = []
    return ["{largest(items: [3, 9, 2]) ?? 0}", largest(items: ["pear", "apple"]) ?? "", "{largest(items: none) ?? -1}"]
}
