// A `where` guard never counts toward exhaustiveness: a guarded `.shipped`
// arm alone leaves `.shipped` uncovered (K2301). An unguarded arm after it
// fixes that.
enum Status {
    case pending
    case shipped(tracking: String, delayed: Bool)
    case cancelled(reason: String)
}

func describe(status: Status) -> String {
    return match status {
        case .shipped(tracking: let tracking, delayed: let delayed) where delayed { tracking }
        case .pending, .cancelled { "-" }
    }
}

func describeAll(status: Status) -> String {
    return match status {
        case .shipped(tracking: let tracking, delayed: let delayed) where delayed { tracking }
        case .shipped { "on its way" }
        case .pending, .cancelled { "-" }
    }
}
