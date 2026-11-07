// The near misses of `tuple-literal-patterns-are-reachable.k`: a tuple arm is
// unreachable when earlier arms cover every value it matches, a repeated
// literal tuple included; and literal elements alone never make a match over
// a non-enumerable element exhaustive.
func label(a: Int, b: Bool) -> String {
    return match (a, b) {
        case (0, true) { "a" }
        case (0, true) { "b" }
        case (_, true) { "c" }
        case (_, false) { "d" }
        case (1, false) { "e" }
    }
}

func covered(a: Int, b: Bool) -> Int {
    return match (a, b) {
        case (0, _) { 0 }
        case (_, _) { 1 }
        case (0, true) { 2 }
    }
}

func partial(a: Int, b: Bool) -> Int {
    return match (a, b) {
        case (0, true) { 0 }
        case (0, false) { 1 }
    }
}
