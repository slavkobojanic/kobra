// A `var` binding may be reassigned and compound-assigned.
func total() -> Int {
    var count = 1
    count = 2
    count += 3
    count *= 2
    return count
         //^^^^^ type: Int
}
