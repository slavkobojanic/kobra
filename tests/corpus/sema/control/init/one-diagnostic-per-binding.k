// After the first uninitialized read is reported, the binding counts as
// initialized: later reads aren't reported again.
func pick() -> Int {
    let a: Int
    let b = a
    let c = a
    return b + c
}
