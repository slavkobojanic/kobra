// K2301 must carry a note listing the missing patterns and a maybe-incorrect fix
// inserting arms for them (diagnostics.md, control flow).
enum Light {
    case red
    case green
    case amber
}

func label(light: Light) -> String {
    return match light {
        case .red { "stop" }
        case .green { "go" }
    }
}
