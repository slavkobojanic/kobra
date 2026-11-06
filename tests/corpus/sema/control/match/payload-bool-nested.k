// A `Bool` inside a payload is enumerable too: `delayed: true` and `delayed:
// false` together cover `.shipped`.
enum Status {
    case pending
    case shipped(tracking: String, delayed: Bool)
    case cancelled(reason: String)
}

func describe(status: Status) -> String {
    return match status {
        case .shipped(tracking: _, delayed: true) { "late" }
        case .shipped(tracking: _, delayed: false) { "on time" }
        case .pending, .cancelled { "-" }
    }
}

func describePartial(status: Status) -> String {
    return match status {
        case .shipped(tracking: _, delayed: true) { "late" }
        case .pending, .cancelled { "-" }
    }
}
