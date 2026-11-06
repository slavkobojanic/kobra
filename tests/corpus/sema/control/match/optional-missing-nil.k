// Without `case nil`, a match over an optional is not exhaustive (K2301).
func describe(count: Int?) -> String {
    return match count {
        case .some(let value) { "{value}" }
    }
}
