import k.charts.ChartScalable

/// A whole number.
///
/// Arithmetic `+ - * / %` is built in; `/` truncates. Overflow and division
/// by zero panic in debug and test builds. Number extensions such as
/// `10.minutes` and `24.usd` are members of `Int` and `Double`.
@Native
public struct Int implements Equatable, Hashable, Comparable, Codable, ConfigValue, ChartScalable {
    /// Parses a decimal integer; `nil` if `text` isn't one.
    static func parse(_ text: String) -> Int?

    /// Formats the number in the locale in effect.
    func formatted(as style: NumberStyle) -> String

    /// The duration of this many milliseconds.
    var milliseconds: Duration
    /// The duration of this many seconds.
    var seconds: Duration
    /// The duration of one second; an alias of `seconds` for `1.second`.
    var second: Duration
    /// The duration of this many minutes.
    var minutes: Duration
    /// An alias of `minutes`.
    var minute: Duration
    /// The duration of this many hours.
    var hours: Duration
    /// An alias of `hours`.
    var hour: Duration
    /// The duration of this many days.
    var days: Duration
    /// An alias of `days`.
    var day: Duration
    /// The duration of this many weeks.
    var weeks: Duration
    /// An alias of `weeks`.
    var week: Duration
    /// This many bytes, as a byte count.
    var bytes: Int
    /// This many kilobytes (1,000 bytes each), as a byte count.
    var kilobytes: Int
    /// This many megabytes (1,000,000 bytes each), as a byte count.
    var megabytes: Int
}
