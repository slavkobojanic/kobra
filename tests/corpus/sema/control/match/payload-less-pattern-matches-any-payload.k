// As in `catch`, a case written without its payload matches every payload.
enum Status {
    case pending
    case shipped(tracking: String, delayed: Bool)
    case cancelled(reason: String)
}

func isOpen(status: Status) -> Bool {
    return match status {
        case .pending, .shipped { true }
        case .cancelled { false }
    }
}
