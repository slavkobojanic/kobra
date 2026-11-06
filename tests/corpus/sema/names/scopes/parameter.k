// A parameter is visible in its function's body under its internal name.
func next(after value: Int) -> Int {
    return value + 1
    //     ^^^^^ resolves: param value 2:17
}
