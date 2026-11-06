// A nested payload left uncovered is K2301: `.light(.amber)` is missing.
enum Light {
    case red
    case amber
    case green
}

enum Signal {
    case light(Light)
    case off
}

func canGo(signal: Signal) -> Bool {
    return match signal {
        case .light(.green) { true }
        case .light(.red) { false }
        case .off { false }
    }
}
