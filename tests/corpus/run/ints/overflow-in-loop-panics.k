// A loop that doubles a value panics at the first result outside the safe range, after logging the last good one.
import k.observe.Log

func main() -> Int {
    var n = 1
    var steps = 0
    while steps < 100 {
        if n > 4503599627370495 {
            Log.info("last {n}")
        }
        n = n * 2
        steps += 1
    }
    return n
}
