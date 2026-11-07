// Patterns nest: a case inside a payload, a literal inside a payload, and `_` for a part that doesn't matter.
enum Speed {
    case express(days: Int)
    case standard
}

enum Shipment {
    case shipped(Speed)
    case pending
}

func eta(shipment: Shipment) -> String {
    return match shipment {
        case .shipped(.express(days: 1)) { "tomorrow" }
        case .shipped(.express(let d)) where d < 3 { "in {d} days" }
        case .shipped(.express(days: _)) { "later" }
        case .shipped(.standard) { "standard" }
        case .pending { "pending" }
    }
}

func main() -> List<String> {
    return [eta(shipment: .shipped(.express(days: 1))), eta(shipment: .shipped(.express(days: 2))), eta(shipment: .shipped(.express(days: 5))), eta(shipment: .shipped(.standard)), eta(shipment: .pending)]
}
