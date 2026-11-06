// A binding in a `while` body is local to one iteration's block.
func countdown(from start: Int) -> Int {
    var remaining = start
    while remaining > 0 {
        let step = 1
        remaining -= step
        //           ^^^^ resolves: local step 5:13
    }
    return step
}
