// Stage 3 finding: a tuple pattern with a literal element marked every later
// arm unreachable (K2302), as if `(0, true)` covered every `(Int, Bool)`. A
// literal element covers only its value: these arms are all reachable, and the
// first match is exhaustive through its wildcard arms.
func label(a: Int, b: Bool) -> String {
    return match (a, b) {
        case (0, true) { "zero yes" }
        case (0, false) { "zero no" }
        case (1, _) { "one" }
        case (_, true) { "yes" }
        case (_, false) { "no" }
    }
}

func pair(a: String, b: Int) -> Int {
    return match (a, b) {
        case ("x", 1) { 1 }
        case ("x", 2) { 2 }
        case ("y", _) { 3 }
        case (_, 1) { 4 }
        default { 0 }
    }
}

enum Light {
    case red
    case green
}

func go(light: Light, count: Int) -> Bool {
    return match (light, count) {
        case (.red, 0) { false }
        case (.red, _) { false }
        case (.green, 0) { true }
        case (.green, _) { true }
    }
}
