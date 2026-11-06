// A `where` guard of another type is K2001 at the guard expression.
enum Light {
    case red
    case amber
    case green
}

func label(light: Light, count: Int) -> String {
    return match light {
        case .red where count { "stop" }
        default { "go" }
    }
}
