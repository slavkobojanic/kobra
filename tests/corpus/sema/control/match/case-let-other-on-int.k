// On a non-enumerable type, `case let other` is the binding catch-all that
// makes the match exhaustive.
func size(count: Int) -> String {
    return match count {
        case 0 { "empty" }
        case let other { "{other} items" }
    }
}
