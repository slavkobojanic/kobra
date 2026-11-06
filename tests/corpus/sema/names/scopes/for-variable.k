// A `for` variable is visible in the loop body only.
func sum(values: List<Int>) -> Int {
    var total = 0
    for value in values {
        total += value
        //       ^^^^^ resolves: local value 4:9
      //^^^^^ resolves: local total 3:9
    }
    return total + value
}
