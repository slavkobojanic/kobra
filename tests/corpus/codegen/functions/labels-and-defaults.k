// Labels are erased and defaults are JS default parameters; an omitted middle argument is passed as
// `undefined` (docs/compiler/codegen.md §4).
func greet(name: String, greeting: String = "Hello", punctuation: String = "!") -> String {
    return "{greeting}, {name}{punctuation}"
}

func both() -> List<String> {
    return [greet(name: "Ana"), greet(name: "Ben", punctuation: "?")]
}
