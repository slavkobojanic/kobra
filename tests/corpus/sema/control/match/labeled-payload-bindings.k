// Labeled payloads bind by label; `_` inside a pattern is fine.
enum Status {
    case pending
    case shipped(tracking: String, delayed: Bool)
    case cancelled(reason: String)
}

func describe(status: Status) -> String {
    return match status {
        case .pending { "pending" }
        case .shipped(tracking: let tracking, delayed: _) { tracking }
//                                  ^^^^^^^^ type: String
        case .cancelled(reason: let reason) { reason }
    }
}
