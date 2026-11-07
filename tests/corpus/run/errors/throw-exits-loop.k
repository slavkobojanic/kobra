// A throw inside a loop leaves the loop and the function at once.
import k.observe.Log

enum Stop implements Error {
    case at(Int)
}

func scan(values: List<Int>) throws Stop -> Int {
    var total = 0
    for v in values {
        if v < 0 {
            throw Stop.at(v)
        }
        Log.info("adding {v}")
        total += v
    }
    return total
}

func main() -> String {
    do {
        return "{try scan(values: [1, 2, -3, 4])}"
    } catch .at(let v) {
        return "stopped at {v}"
    }
}
