// `sum()` on `List<Double>`; interpolating a whole Double keeps one decimal.
import k.observe.Log

func main() -> Double {
    let weights = [0.5, 1.5, 2.0]
    let total = weights.sum()
    Log.info("total {total}")
    let empty: List<Double> = []
    Log.info("empty {empty.sum()}")
    return total
}
