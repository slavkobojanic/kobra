// A generic enum carries any payload type.
enum Outcome<T> {
    case ok(T)
    case none
}

func describe(outcome: Outcome<Int>) -> String {
    return match outcome {
        case .ok(let v) { "ok {v}" }
        case .none { "none" }
    }
}

func main() -> List<String> {
    let a: Outcome<Int> = .ok(3)
    let b: Outcome<Int> = .none
    return [describe(outcome: a), describe(outcome: b)]
}
