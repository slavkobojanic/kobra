// A `where` guard sees the arm's bindings, and must be a `Bool`.
enum Status {
    case pending
    case shipped(tracking: String, delayed: Bool)
    case cancelled(reason: String)
}

func describe(status: Status) -> String {
    return match status {
        case .cancelled(reason: let reason) where reason.isEmpty { "cancelled" }
//                                                ^^^^^^ resolves: local reason 10:37
        case .cancelled(reason: let reason) { reason }
        default { "-" }
    }
}
