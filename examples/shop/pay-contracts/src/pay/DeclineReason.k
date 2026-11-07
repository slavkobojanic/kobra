/// Why a card was declined.
public enum DeclineReason implements Codable, Equatable {
    /// The issuer declined without saying why (`4000 0000 0000 0002`).
    case cardDeclined

    /// The account can't cover the amount (`4000 0000 0000 9995`).
    case insufficientFunds

    /// The card has expired (`4000 0000 0000 0069`).
    case expiredCard
}
