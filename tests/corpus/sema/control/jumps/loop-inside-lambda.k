// A loop inside a lambda is fine: `break` leaves that inner loop.
func scan(groups: List<List<Int>>) -> List<Int> {
    return groups.map { group =>
        var total = 0
        for value in group {
            if value < 0 {
                break
            }
            total += value
        }
        total
    }
}
