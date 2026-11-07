// Mutating an inner list through a copy of the outer list leaves the original untouched.
func main() -> List<List<List<Int>>> {
    let grid = [[1], [2]]
    var copy = grid
    copy[0].append(9)
    var rows: List<List<Int>> = []
    for i in 0..<2 {
        var row: List<Int> = []
        row.append(i)
        rows.append(row)
    }
    return [grid, copy, rows]
}
