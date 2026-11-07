import k.crypto.Uuid

/// A verified webhook event, reduced to what the shop acts on.
public enum PaymentEvent implements Equatable {
    /// A checkout started by this shop was paid in full.
    ///
    /// - `orderId`: the order named by the checkout's `client_reference_id`.
    /// - `amount`: the amount collected.
    /// - `reference`: Stripe's checkout session id.
    case checkoutPaid(orderId: Uuid, amount: Money, reference: String)

    /// An event the shop doesn't act on: another event type, a checkout that
    /// isn't paid yet, or one this shop didn't start. It is acknowledged so
    /// Stripe stops redelivering it.
    case ignored(type: String)
}
