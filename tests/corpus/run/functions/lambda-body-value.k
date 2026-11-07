// A lambda's value is its last expression, after the statements before it run.
import k.observe.Log

func main() -> List<String> {
    return [1, 2].map { n =>
        Log.info("mapping {n}")
        let doubled = n * 2
        "{n}->{doubled}"
    }
}
