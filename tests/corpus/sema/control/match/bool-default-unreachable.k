// After `true` and `false`, a `default` can never run (K2302).
func word(flag: Bool) -> String {
    return match flag {
        case true { "yes" }
        case false { "no" }
        default { "?" }
    }
}
