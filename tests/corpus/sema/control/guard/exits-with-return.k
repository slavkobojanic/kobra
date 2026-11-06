// A `guard let` whose `else` returns; the binding stays in scope after the
// `guard` with the unwrapped type.
func double(value: Int?) -> Int {
    guard let n = value else {
        return 0
    }
    return n * 2
//         ^ resolves: local n 4:15
//         ^ type: Int
}
