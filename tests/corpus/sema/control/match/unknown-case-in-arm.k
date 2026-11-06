// An unknown case in an arm is K2006; an arm with an error doesn't feed the
// exhaustiveness check, so no K2301 follows.
enum Light {
    case red
    case amber
    case green
}

func canGo(light: Light) -> Bool {
    return match light {
        case .red, .amber { false }
        case .gren { true }
    }
}
