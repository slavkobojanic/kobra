// A match on a value of unresolved type checks nothing more: no K2006 for its patterns, no K2301.
func code(light: Lite) -> Int {
    return match light {
        case .red { 1 }
    }
}
