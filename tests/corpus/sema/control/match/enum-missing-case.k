// A missing enum case is K2301 at the `match` keyword.
enum Light {
    case red
    case amber
    case green
}

func label(light: Light) -> String {
    return match light {
        case .red { "stop" }
        case .green { "go" }
    }
}
