// Log lines before an uncaught throw are kept.
import k.observe.Log

enum Failure implements Error, Codable {
    case broken(reason: String, code: Int)
}

func main() throws Failure {
    Log.info("starting")
    Log.warning("about to fail")
    throw Failure.broken(reason: "test", code: 7)
}
