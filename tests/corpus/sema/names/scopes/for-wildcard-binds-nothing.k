// `for _ in` binds no name: `_` can't be read.
func count(values: List<Int>) -> Int {
    var total = 0
    for _ in values {
        total += 1
    }
    return total
    //     ^^^^^ resolves: local total 3:9
}
