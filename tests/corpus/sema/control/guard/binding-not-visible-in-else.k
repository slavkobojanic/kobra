// The `guard let` binding is not in scope inside its own `else` block (K1201).
func double(value: Int?) -> Int {
    guard let n = value else {
        return n
    }
    return n * 2
}
