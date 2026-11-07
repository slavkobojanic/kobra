// An `if` expression arm may run statements before its value.
import k.observe.Log

func main() -> String {
    let n = 4
    let label = if n > 3 {
        Log.info("big branch")
        "big"
    } else {
        Log.info("small branch")
        "small"
    }
    return label
}
