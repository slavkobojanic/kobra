// Inside a loop, `break` and `continue` exit the `guard`'s scope.
func sum(values: List<Int?>) -> Int {
    var total = 0
    for value in values {
        guard let n = value else {
            continue
        }
        guard n >= 0 else {
            break
        }
        total += n
    }
    return total
}
