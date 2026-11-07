// Half-open and closed range patterns, negative bounds included; boundaries go to the right arm.
func bucket(n: Int) -> String {
    return match n {
        case -10..<0 { "negative" }
        case 0..<10 { "small" }
        case 10...99 { "medium" }
        default { "other" }
    }
}

func main() -> List<String> {
    return [bucket(n: -10), bucket(n: -1), bucket(n: 0), bucket(n: 9), bucket(n: 10), bucket(n: 99), bucket(n: 100), bucket(n: -11)]
}
