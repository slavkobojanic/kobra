// Top-level `let` constants are evaluated once and visible to every function.
import k.observe.Log

let limit = computeLimit()

func computeLimit() -> Int {
    Log.info("computing")
    return 3
}

func main() -> List<Int> {
    return [limit, limit * 2]
}
