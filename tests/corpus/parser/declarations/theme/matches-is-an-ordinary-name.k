// `matches` is a keyword only after a variant's name.
theme T {
    variant matches {
        matches = #000
    }
}
func f() -> Bool {
    return matches(x)
}
