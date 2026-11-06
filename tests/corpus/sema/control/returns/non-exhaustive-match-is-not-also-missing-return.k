// A `match` that returns in every arm but misses a case is K2301; the end of
// the function isn't also reported as K2307 (one error, not two).
enum Light {
    case red
    case amber
    case green
}

func label(light: Light) -> String {
    match light {
        case .red { return "stop" }
        case .green { return "go" }
    }
}
