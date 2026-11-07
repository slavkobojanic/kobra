// `if` and `match` are expressions; there are no ternaries.
func label(count: Int) -> String {
    let noun = if count == 1 { "item" } else { "items" }
    let size = match count {
        case 0 { "none" }
        case 1, 2, 3 { "few" }
        default { "many" }
    }
    return "{count} {noun} ({size})"
}

func main() -> List<String> {
    return [label(count: 0), label(count: 1), label(count: 7)]
}
