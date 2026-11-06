// Patterns nest: an enum payload of enum type is covered case by case.
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
        case .light(.red), .light(.amber) { false }
        case .off { false }
    }
}
