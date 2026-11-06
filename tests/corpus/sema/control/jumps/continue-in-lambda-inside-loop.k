// The same for `continue`.
func scan(groups: List<List<Int>>) -> Int {
    var total = 0
    for group in groups {
        let kept = group.filter { value =>
            if value < 0 {
                continue
            }
            true
        }
        total += kept.count
    }
    return total
}
