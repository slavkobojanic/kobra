// The matched expression is evaluated once, however many arms are tried.
import k.observe.Log

func next() -> Int {
    Log.info("evaluated")
    return 42
}

func main() -> String {
    return match next() {
        case 1 { "one" }
        case 2 { "two" }
        default { "other" }
    }
}
