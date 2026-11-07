// Log lines before a panic are kept; nothing after it runs.
import k.observe.Log

func main() -> Int {
    Log.info("before")
    let values: List<Int> = []
    let n = values[0]
    Log.info("after {n}")
    return n
}
