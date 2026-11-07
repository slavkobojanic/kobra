// Int overflow panics in debug and test builds (docs/compiler/codegen.md §5).
func main() -> Int {
    let big = 9007199254740991
    return big + 1
}
