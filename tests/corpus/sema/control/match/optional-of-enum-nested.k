// Nested patterns over an optional enum: `.some(.red)` covers one value only.
enum Light {
    case red
    case amber
    case green
}

func label(light: Light?) -> String {
    return match light {
        case nil { "off" }
        case .some(.red) { "stop" }
        case .some(.amber), .some(.green) { "go" }
    }
}

func labelPartial(light: Light?) -> String {
    return match light {
        case nil { "off" }
        case .some(.red) { "stop" }
    }
}
