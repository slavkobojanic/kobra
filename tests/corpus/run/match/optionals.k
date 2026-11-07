// Optionals match against `nil` and `.some`, with literal payloads checked first.
func describe(value: Int?) -> String {
    return match value {
        case nil { "none" }
        case .some(0) { "zero" }
        case .some(let v) { "some {v}" }
    }
}

func main() -> List<String> {
    return [describe(value: nil), describe(value: 0), describe(value: 7)]
}
