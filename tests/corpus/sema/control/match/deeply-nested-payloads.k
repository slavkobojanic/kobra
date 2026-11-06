// Three levels of nesting are checked like one.
enum Light {
    case red
    case amber
    case green
}

enum Signal {
    case light(Light)
    case off
}

enum Crossing {
    case signal(Signal)
    case closed
}

func canGo(crossing: Crossing) -> Bool {
    return match crossing {
        case .signal(.light(.green)) { true }
        case .signal(.light(.red)), .signal(.light(.amber)) { false }
        case .signal(.off), .closed { false }
    }
}

func canGoPartial(crossing: Crossing) -> Bool {
    return match crossing {
        case .signal(.light(.green)) { true }
        case .signal(.off), .closed { false }
    }
}
