// `Bool` is enumerable: `true` and `false` are exhaustive.
func word(flag: Bool) -> String {
    return match flag {
        case true { "yes" }
        case false { "no" }
    }
}
