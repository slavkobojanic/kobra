// Range patterns are never exhaustive, even when they seem to cover every
// value: `default` is required.
func bucket(count: Int) -> String {
    return match count {
        case -1000...-1 { "negative" }
        case 0..<10 { "small" }
        case 10...1000 { "large" }
    }
}

func bucketWithDefault(count: Int) -> String {
    return match count {
        case 0 { "empty" }
        case 1..<10 { "small" }
        default { "large" }
    }
}
