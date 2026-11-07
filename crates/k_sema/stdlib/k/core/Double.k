import k.charts.ChartScalable

/// A 64-bit floating-point number.
@Native
public struct Double implements Equatable, Comparable, Codable, ConfigValue, ChartScalable {
    /// Parses a decimal number such as `"2.5"`.
    ///
    /// - Returns: the nearest `Double`, or `nil` if `text` isn't a number.
    static func parse(_ text: String) -> Double?

    /// Returns the number rounded to the nearest whole number.
    func rounded() -> Int

    /// Formats the number in the locale in effect, as `style` says
    /// ([`NumberStyle`]).
    func formatted(as style: NumberStyle) -> String

    /// The duration of this many seconds (`0.2.seconds`).
    var seconds: Duration
    /// The duration of this many minutes.
    var minutes: Duration
    /// The duration of this many hours.
    var hours: Duration
}
