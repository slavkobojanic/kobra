// `*` is checked too: 2^52 * 2 is 2^53, one past the largest safe integer.
func main() -> Int {
    let half = 4503599627370496
    return half * 2
}
