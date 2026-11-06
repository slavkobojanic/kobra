// A positional (`_`) parameter is bound by its internal name too.
func double(_ value: Int) -> Int {
    return value * 2
    //     ^^^^^ resolves: param value 2:15
}
