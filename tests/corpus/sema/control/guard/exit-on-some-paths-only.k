// Exiting on only some paths isn't enough: the `if` without `else` can fall
// through (K2305).
func double(value: Int?, strict: Bool) -> Int {
    guard let n = value else {
        if strict {
            return -1
        }
    }
    return n * 2
}
