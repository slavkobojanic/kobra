/// Which `pay.PaymentEvents` rpc an `EventDelivery` calls.
public enum EventKind implements Codable, Equatable {
    /// `chargeSucceeded(charge:)`.
    case chargeSucceeded

    /// `chargeRefunded(refund:)`.
    case chargeRefunded
}
