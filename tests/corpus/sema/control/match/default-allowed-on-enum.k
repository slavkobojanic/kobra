// `default` is allowed on an enum, as a deliberate trade-off.
enum Light {
    case red
    case amber
    case green
}

func canGo(light: Light) -> Bool {
    return match light {
        case .green { true }
        default { false }
    }
}
