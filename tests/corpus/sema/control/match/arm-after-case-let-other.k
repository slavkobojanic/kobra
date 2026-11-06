// Like `default`, `case let other` must be last: a later arm is unreachable.
enum Light {
    case red
    case amber
    case green
}

func describe(light: Light) -> String {
    return match light {
        case .red { "stop" }
        case let other { "{other}" }
        case .green { "go" }
    }
}
