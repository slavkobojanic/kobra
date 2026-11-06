import k.crypto.Uuid

/// An order as the browser sees it, on the receipt page and in the admin
/// table.
public struct OrderSummary implements Codable, Equatable {
    /// The order's id.
    public let id: Uuid

    /// The name of the product ordered.
    public let productName: String

    /// The number of units ordered.
    public let quantity: Int

    /// The amount charged.
    public let total: Money

    /// Where the receipt and confirmation email go.
    public let email: String

    /// Where the order is in its lifecycle.
    public let status: OrderStatus

    /// When the order was placed.
    public let placedAt: Date
}
