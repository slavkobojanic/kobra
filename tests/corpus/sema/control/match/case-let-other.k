// `case let other` binds the whole value and handles everything left; it
// has the subject's type.
enum Status {
    case pending
    case shipped(tracking: String, delayed: Bool)
    case cancelled(reason: String)
}

func describe(status: Status) -> String {
    return match status {
        case .pending { "pending" }
        case let other { "{other}" }
//               ^^^^^ type: corpus.Status
    }
}
