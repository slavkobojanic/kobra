// `if let` with shorthand, several bindings and a condition; all must hold.
func pair(a: Int?, b: Int?) -> String {
    if let a, let b, a < b {
        return "{a} < {b}"
    }
    if let a {
        return "only {a}"
    }
    return "none"
}

func main() -> List<String> {
    return [pair(a: 1, b: 2), pair(a: 3, b: 2), pair(a: 3, b: nil), pair(a: nil, b: 2)]
}
