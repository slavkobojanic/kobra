// `default` must be last: an arm after it can never run (K2302).
enum Light {
    case red
    case amber
    case green
}

func canGo(light: Light) -> Bool {
    return match light {
        case .green { true }
        default { false }
        case .red { false }
    }
}
