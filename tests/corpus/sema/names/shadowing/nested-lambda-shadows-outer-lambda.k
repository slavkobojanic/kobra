// An inner lambda's parameter shadows the outer lambda's parameter of the same name.
func incremented(rows: List<List<Int>>) -> List<List<Int>> {
    return rows.map { row => row.map { row => row + 1 } }
    //                       ^^^ resolves: local row 3:23
    //                                        ^^^ resolves: local row 3:40
}
