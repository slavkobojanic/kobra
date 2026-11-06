// A `match` over an enum from another module is exhaustive over all of its
// cases; qualified patterns name the imported enum.
import model.Status

/// Labels for statuses.
public struct Badge {
    let status: Status

    func label() -> String {
        return match status {
            case .active { "on" }
            case .paused { "off" }
        }
    }

    func fullLabel() -> String {
        return match status {
            case Status.active { "on" }
            case Status.paused, Status.archived { "off" }
        }
    }
}
