// A pattern naming a case the enum lacks is K2006; `default` keeps the match exhaustive, so nothing else is reported.
enum Light {
    case red
    case amber
    case green
}

func isStop(light: Light) -> Bool {
    return match light {
        case .purple { true }
        default { false }
    }
}
