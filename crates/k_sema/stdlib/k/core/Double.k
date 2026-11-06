import k.charts.ChartScalable

/// A 64-bit floating-point number.
@Native
public struct Double implements Equatable, Comparable, Codable, ConfigValue, ChartScalable {
    /// Parses a decimal number; `nil` if `text` isn't one.
    static func parse(_ text: String) -> Double?

    /// The number rounded to the nearest whole number.
    func rounded() -> Int

    /// Formats the number in the locale in effect.
    func formatted(as style: NumberStyle) -> String

    /// The duration of this many seconds (`0.2.seconds`).
    var seconds: Duration
    /// The duration of this many minutes.
    var minutes: Duration
    /// The duration of this many hours.
    var hours: Duration
}
