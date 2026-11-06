// A `default` after arms that already cover every case is K2302 at the
// `default` keyword.
enum Light {
    case red
    case amber
    case green
}

func canGo(light: Light) -> Bool {
    return match light {
        case .red, .amber { false }
        case .green { true }
        default { false }
    }
}
