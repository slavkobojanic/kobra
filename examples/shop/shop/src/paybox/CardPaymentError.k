import k.rpc.RpcFailure

/// Why `CardProtocol.pay(orderId:card:)` didn't take the payment.
public enum CardPaymentError implements Error, Codable {
    /// No order has this id.
    case orderNotFound

    /// The order isn't awaiting payment any more: it was paid already, or
    /// cancelled.
    case notAwaitingPayment

    /// Paybox declined the card; `message` says why, for the customer.
    case declined(message: String)

    /// The number isn't a card number.
    case invalidCard

    /// Paybox couldn't be reached, didn't answer in time, or refused the
    /// shop. Nothing was charged.
    case paymentsUnavailable

    /// The call failed outside the handler: the form failed validation on
    /// the server, the client exceeded the rate limit, or the browser
    /// couldn't reach the shop.
    case rpcFailure(RpcFailure)
}
