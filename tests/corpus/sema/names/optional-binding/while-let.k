// `while let` binds in the loop body on each iteration.
func drain(stack: List<Int>) -> Int {
    var rest = stack
    var total = 0
    while let top = rest.last {
        total += top
        //       ^^^ resolves: local top 5:15
        rest.removeAll()
    }
    return total
}
