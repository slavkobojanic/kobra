// `append`, `insert(at:)`, `removeWhere`, `remove(at:)` and `removeAll` on a `var` list.
func main() -> List<List<Int>> {
    var values = [1, 2, 3]
    values.append(4)
    values.insert(0, at: 0)
    values.insert(9, at: 5)
    let afterInsert = values
    values.removeWhere { v => v % 2 == 0 }
    let afterRemoveWhere = values
    let removed = values.remove(at: 1)
    let afterRemove = values
    values.removeAll()
    return [afterInsert, afterRemoveWhere, [removed], afterRemove, values]
}
