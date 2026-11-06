// Literal and range patterns in a view match.
component T {
    view {
        match count {
            case 0 { "none" }
            case 1...9 { "a few" }
            default { "many" }
        }
    }
}
