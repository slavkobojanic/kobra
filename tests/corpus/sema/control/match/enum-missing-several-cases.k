// Several missing cases are one K2301, not one per case.
enum Light {
    case red
    case amber
    case green
}

func label(light: Light) -> String {
    return match light {
        case .red { "stop" }
    }
}
