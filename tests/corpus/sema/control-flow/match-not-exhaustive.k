enum Light {
    case red
    case green
    case amber
}

func label(light: Light) -> String {
    match light {
        case .red {
            return "stop"
        }
        case .green {
            return "go"
        }
    }
    return "?"
}
