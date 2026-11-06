// Arguments of an unresolved call are still resolved: their own errors are reported, and nothing else.
func run(value: Int) -> Int {
    return missing(value, other)
    //             ^^^^^ resolves: param value 2:10
}
