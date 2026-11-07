/// The payload of `EventDeliveryJob`: one event for one merchant.
///
/// It holds only ids. The job reads the charge or refund when it runs, so a
/// late delivery carries the current state, such as a charge's refunded
/// amount.
public struct EventDelivery implements Codable, Equatable {
    /// The event to deliver.
    let kind: EventKind

    /// The merchant to deliver it to.
    let merchant: String

    /// The charge the event is about; for a refund, the refund's id.
    let subject: String
}
