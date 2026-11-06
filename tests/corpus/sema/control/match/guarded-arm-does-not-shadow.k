// A guarded arm doesn't make a later arm for the same case unreachable.
enum Light {
    case red
    case amber
    case green
}

func canGo(light: Light, late: Bool) -> Bool {
    return match light {
        case .amber where late { false }
        case .amber { true }
        case .red { false }
        case .green { true }
    }
}
