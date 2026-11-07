// `List + List` is an array spread; `String + String` stays `+` (docs/compiler/codegen.md §6.1).
func joined(a: List<Int>, b: List<Int>) -> List<Int> {
    return a + b
}

func greeting(name: String) -> String {
    return "Hello, " + name
}
