// An `else` block that can fall through is K2305 at the `else` keyword.
func double(value: Int?) -> Int {
    guard let n = value else {
        let fallback = 0
    }
    return n * 2
}
