// In a multi-pattern arm, only the pattern that can never match is reported.
enum Light {
    case red
    case amber
    case green
}

func canGo(light: Light) -> Bool {
    return match light {
        case .red { false }
        case .amber, .red { false }
        case .green { true }
    }
}
