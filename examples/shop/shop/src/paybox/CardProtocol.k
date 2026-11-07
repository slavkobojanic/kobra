import k.auth.RateLimit
import k.crypto.Uuid

/// The `Payment` page's contract with the shop's server.
///
/// The browser never calls paybox: a remote client holds a service token,
/// so only the shop's server may hold one. The server charges the card
/// through paybox, and the payment is confirmed when paybox delivers its
/// `chargeSucceeded` event to `PaymentEventsApi`.
public protocol CardProtocol {
    /// Charges an order's total to a card.
    ///
    /// - Parameters:
    ///   - orderId: the order to pay for.
    ///   - card: the validated card form.
    /// - Throws: `CardPaymentError.orderNotFound` or `.notAwaitingPayment`
    ///   when there is nothing to pay, `.declined` or `.invalidCard` for a
    ///   card paybox refuses, `.paymentsUnavailable` when paybox fails, and
    ///   `.rpcFailure` when the request is invalid, the client exceeds 10
    ///   attempts a minute, or the browser can't reach the shop.
    rpc pay(orderId: Uuid, card: CardForm) throws CardPaymentError requires RateLimit(10.perMinute)
}
