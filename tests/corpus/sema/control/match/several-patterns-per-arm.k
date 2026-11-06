// Patterns separated by commas share an arm; together the arms cover the enum.
enum Light {
    case red
    case amber
    case green
}

func canGo(light: Light) -> Bool {
    return match light {
        case .red, .amber { false }
        case .green { true }
    }
}
