// A `default` after literal arms over `Int` is always reachable: no K2302.
func size(count: Int) -> String {
    return match count {
        case 0 { "empty" }
        default { "some" }
    }
}
