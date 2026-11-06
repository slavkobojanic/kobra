// An unused `case let other` is K2304 at the binding pattern, with a
// machine-applicable fix replacing the arm head with `default`.
enum Status {
    case pending
    case shipped(tracking: String, delayed: Bool)
    case cancelled(reason: String)
}

func describe(status: Status) -> String {
    return match status {
        case .pending { "pending" }
        case let other { "other" }
    }
}
