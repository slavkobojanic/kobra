// A non-`Void` function that can reach its end is K2307 at the body's closing
// brace.
func sign(n: Int) -> Int {
    if n > 0 {
        return 1
    } else if n < 0 {
        return -1
    }
}
