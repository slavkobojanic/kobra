// Enums may hold methods, static members and computed properties next to cases.
enum Currency {
    case usd
    case eur

    static let fallback: Currency = .usd

    var symbol: String {
        match self {
            case .usd { "$" }
            case .eur { "€" }
        }
    }

    func format(amount: Int) -> String {
        return "{symbol}{amount}"
    }
}
