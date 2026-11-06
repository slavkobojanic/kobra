// An unresolved callee is K1201 only: whether it throws or is async is
// unknown, so there is no K2201 or K2204, and no type error for its result.
func show() async -> Int {
    let value = fetchCount()
    return value + 1
}
