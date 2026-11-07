/// How `formatted(as:)` ([`Int.formatted`], [`Double.formatted`],
/// [`Money.formatted`]) writes a number or an amount of money.
public enum NumberStyle {
    /// Digits with the locale's grouping and decimal separators.
    case decimal
    /// Rounded to a whole number.
    case wholeNumber
    /// A percentage of 1 (`0.25` is "25%").
    case percent
    /// An amount of money in its own currency, in the locale in effect.
    case currency
}
