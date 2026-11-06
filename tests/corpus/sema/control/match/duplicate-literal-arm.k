// A literal arm repeated is unreachable: K2302 at the second pattern.
func size(count: Int) -> String {
    return match count {
        case 0 { "empty" }
        case 1 { "one" }
        case 0 { "zero" }
        default { "many" }
    }
}
