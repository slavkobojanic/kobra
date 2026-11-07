import pay.DeclineReason

/// What paybox does with a card number. See `TestCards.outcome(number:)`.
public enum CardOutcome implements Equatable {
    /// The charge succeeds.
    case approved

    /// The card is declined, for `reason`.
    case declined(reason: DeclineReason)

    /// The number isn't a card number.
    case invalid
}
