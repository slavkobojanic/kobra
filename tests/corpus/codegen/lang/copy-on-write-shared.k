// Mutating a list that is also held elsewhere copies it first (docs/compiler/codegen.md §6.3).
func appended(list: List<Int>, value: Int) -> List<Int> {
    var copy = list
    copy.append(value)
    return copy
}
