// Static parts of an interpolated string escape backticks, backslashes and `${`
// (docs/compiler/codegen.md §6.4).
func tricky(name: String) -> String {
    return "`{name}` costs ${{1} \\ end"
}
