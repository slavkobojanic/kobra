// Doubles interpolate in the shortest round-trip form with at least one decimal.
import k.observe.Log

func main() {
    let third = 1.0 / 3.0
    let sum = 0.1 + 0.2
    Log.info("{2.0} {0.1} {100.0} {-0.5}")
    Log.info("{third}")
    Log.info("{sum}")
    Log.info("{1234567.5}")
}
