// A function whose last statement calls an unresolved function: the call might
// return `Never`, so its end is not reported as K2307. K1201 only.
func sign(n: Int) -> Int {
    if n > 0 {
        return 1
    }
    fail()
}
