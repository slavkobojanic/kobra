// `match` on a Bool is exhaustive with `true` and `false`.
func word(flag: Bool) -> String {
    return match flag {
        case true { "yes" }
        case false { "no" }
    }
}

func main() -> List<String> {
    return [word(flag: true), word(flag: false), word(flag: 1 > 2)]
}
