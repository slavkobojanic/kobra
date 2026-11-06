// A top-level `case _` is K2303 from `case` to `_`, with a machine-applicable
// fix to `default`.
enum Light {
    case red
    case amber
    case green
}

func canGo(light: Light) -> Bool {
    return match light {
        case .green { true }
        case _ { false }
    }
}
