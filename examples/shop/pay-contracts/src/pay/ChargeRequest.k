/// What to charge.
public struct ChargeRequest implements Codable {
    /// The merchant's own reference for the payment, such as its order id.
    /// Paybox charges each reference of a merchant at most once.
    let reference: String

    /// The amount to charge. It must be positive.
    let amount: Money

    /// The card to charge.
    let card: CardDetails

    /// What the customer is paying for, as it would appear on a statement.
    let description: String
}
