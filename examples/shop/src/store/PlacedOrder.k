import k.crypto.Uuid

/// The result of placing an order: its id, and where to send the customer
/// to pay.
public struct PlacedOrder implements Codable {
    /// The new order's id.
    public let orderId: Uuid

    /// The payment page to redirect the customer to. It belongs to the
    /// payment provider in production and is the receipt page elsewhere; see
    /// `PaymentGateway`.
    public let paymentUrl: Url
}
