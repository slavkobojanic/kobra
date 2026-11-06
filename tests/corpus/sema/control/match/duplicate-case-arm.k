// An arm whose case an earlier arm already handles is K2302 at its pattern.
enum Light {
    case red
    case amber
    case green
}

func canGo(light: Light) -> Bool {
    return match light {
        case .red { false }
        case .green { true }
        case .red { true }
        case .amber { false }
    }
}
