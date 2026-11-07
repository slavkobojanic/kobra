// Lists have value semantics: appending to a copy leaves the original unchanged, however the
// copy was made (docs/compiler/codegen.md §6.3).
func appended(list: List<Int>, value: Int) -> List<Int> {
    var copy = list
    copy.append(value)
    return copy
}

func main() -> List<List<Int>> {
    let original = [1, 2]
    var alias = original
    alias.append(3)
    let grown = appended(list: original, value: 4)
    var built: List<Int> = []
    for i in 0..<3 {
        built.append(i)
    }
    return [original, alias, grown, built]
}
