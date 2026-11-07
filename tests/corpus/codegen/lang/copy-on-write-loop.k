// A loop appending to a fresh local list mutates it in place: it is owned, so no copy is made
// (docs/compiler/codegen.md §6.3).
func squares(n: Int) -> List<Int> {
    var out: List<Int> = []
    for i in 0..<n {
        out.append(i * i)
    }
    return out
}
