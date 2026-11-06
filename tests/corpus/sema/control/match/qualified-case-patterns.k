// A qualified case pattern (`Light.red`) is the same as `.red`.
enum Light {
    case red
    case amber
    case green
}

func canGo(light: Light) -> Bool {
    return match light {
        case Light.green { true }
        case Light.red, .amber { false }
    }
}
