// Interpolation formats each part by its type: Double through `$k.formatDouble`, Int and Bool by
// `String(…)`, String as is (docs/compiler/codegen.md §6.4).
func line(name: String, count: Int, ratio: Double, done: Bool) -> String {
    return "{name}: {count} ({ratio}) {done}"
}
