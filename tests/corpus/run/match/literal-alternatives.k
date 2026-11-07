// One arm can list several literals; `default` takes the rest.
func kind(n: Int) -> String {
    return match n {
        case 0 { "zero" }
        case 1, 3, 5, 7, 9 { "odd digit" }
        case 2, 4, 6, 8 { "even digit" }
        default { "big" }
    }
}

func main() -> List<String> {
    return [kind(n: 0), kind(n: 7), kind(n: 4), kind(n: 10), kind(n: -1)]
}
