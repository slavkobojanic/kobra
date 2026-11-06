// An inner lambda sees the outer lambda's parameters.
func products(rows: List<Int>, cols: List<Int>) -> List<List<Int>> {
    return rows.map { row => cols.map { col => row * col } }
    //                                         ^^^ resolves: local row 3:23
    //                                               ^^^ resolves: local col 3:41
}
