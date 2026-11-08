/// A time of day such as `TimeOfDay("03:00")`: a job schedule's time, or
/// any value that names a time on a 24-hour clock, such as opening hours.
///
/// It is its `HH:MM` text on both sides, so it is printed and encoded as
/// that text, and equal and hashable by it.
@Native
public struct TimeOfDay implements Equatable, Hashable, Codable {
    /// A literal `"HH:MM"` on a 24-hour clock, validated at compile time.
    @Native
    @Literal
    init(_ literal: String) {}

    /// Parses `HH:MM` on a 24-hour clock, two digits each, by the rule a
    /// `TimeOfDay("…")` literal is checked with.
    ///
    /// - Returns: the time of day, or `nil` for any other text, such as
    ///   `"24:00"`, `"7:05"` or `"07:05:00"`.
    static func parse(_ text: String) -> TimeOfDay?

    /// The hour, `0...23`.
    var hour: Int
    /// The minute, `0...59`.
    var minute: Int
}
