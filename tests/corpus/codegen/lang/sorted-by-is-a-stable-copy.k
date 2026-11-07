// `sortedBy` sorts a copy, so the receiver is never mutated (docs/compiler/codegen.md §6.6).
func ordered(values: List<Int>) -> List<Int> {
    return values.sortedBy { v => -v }
}
