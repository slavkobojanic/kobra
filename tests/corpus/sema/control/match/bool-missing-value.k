// A `Bool` match without `false` (or `default`) is K2301.
func word(flag: Bool) -> String {
    return match flag {
        case true { "yes" }
    }
}
