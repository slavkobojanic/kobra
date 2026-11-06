// A lambda is a boundary: `break` inside a lambda can't leave a loop outside
// it (K2309), even though the lambda is written inside the loop.
func scan(groups: List<List<Int>>) -> Int {
    var total = 0
    for group in groups {
        let doubled = group.map { value =>
            if value < 0 {
                break
            }
            value * 2
        }
        total += doubled.count
    }
    return total
}
