/// How `Date.formatted(as:)` writes a date.
public enum DateStyle {
    /// The date only.
    case date
    /// The time only.
    case time
    /// The date and the time.
    case dateTime
    /// Relative to a reference instant: "5 minutes ago", "in 3 days".
    case relative
}
