/// An exact decimal number, for rates and amounts that must not round.
///
/// Literals with a fraction become `Decimal` where a `Decimal` is expected:
/// `total.multiplied(by: 1.0825, rounding: .halfEven)`.
@Native
public struct Decimal implements Equatable, Hashable, Comparable, Codable, ConfigValue {
    /// Parses a decimal number; `nil` if `text` isn't one.
    static func parse(_ text: String) -> Decimal?
}
