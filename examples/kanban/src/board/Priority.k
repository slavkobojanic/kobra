/// How urgent a card is.
public enum Priority implements Codable, Equatable {
    /// Can wait.
    case low
    /// The default.
    case normal
    /// Needs attention first.
    case high

    /// The word shown on the card.
    var label: String {
        return match self {
            case .low { "Low" }
            case .normal { "Normal" }
            case .high { "High" }
        }
    }
}
