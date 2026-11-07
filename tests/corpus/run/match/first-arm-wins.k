// When several arms match, the first one in source order runs.
func label(n: Int) -> String {
    return match n {
        case 0..<100 { "range" }
        case 5 { "five" }
        default { "other" }
    }
}

func main() -> String {
    return label(n: 5)
}
