// `_` inside a pattern stays valid: `.shipped(tracking: _, delayed: true)`.
enum Status {
    case pending
    case shipped(tracking: String, delayed: Bool)
    case cancelled(reason: String)
}

func isLate(status: Status) -> Bool {
    return match status {
        case .shipped(tracking: _, delayed: true) { true }
        default { false }
    }
}
