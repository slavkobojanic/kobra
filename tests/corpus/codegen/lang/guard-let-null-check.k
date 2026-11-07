// `guard let` compares with `null` (docs/compiler/codegen.md §6.2).
func first(values: List<Int>) -> Int {
    guard let value = values.first else {
        return -1
    }
    return value
}
