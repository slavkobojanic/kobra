// `case let other` after arms that cover every case is unreachable (K2302 at
// its pattern), whether or not it uses the binding.
enum Light {
    case red
    case amber
    case green
}

func describe(light: Light) -> String {
    return match light {
        case .red, .amber { "stop" }
        case .green { "go" }
        case let other { "{other}" }
    }
}
