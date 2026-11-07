// A `match` over a tuple with literal elements (sema regression
// `tuple-literal-patterns-are-reachable.k`) picks the first arm whose every
// element matches.
func label(a: Int, b: Bool) -> String {
    return match (a, b) {
        case (0, true) { "zy" }
        case (0, false) { "zn" }
        case (1, _) { "one" }
        case (_, true) { "y" }
        case (_, false) { "n" }
    }
}

func main() -> String {
    return "{label(a: 0, b: true)} {label(a: 0, b: false)} {label(a: 1, b: false)} {label(a: 2, b: true)} {label(a: 2, b: false)}"
}
