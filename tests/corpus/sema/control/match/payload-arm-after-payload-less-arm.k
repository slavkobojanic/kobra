// `.shipped` without a payload handles every `.shipped(…)`, so a later
// `.shipped(…)` arm is unreachable.
enum Status {
    case pending
    case shipped(tracking: String, delayed: Bool)
    case cancelled(reason: String)
}

func describe(status: Status) -> String {
    return match status {
        case .shipped { "shipped" }
        case .shipped(tracking: let tracking, delayed: true) { tracking }
        case .pending, .cancelled { "-" }
    }
}
