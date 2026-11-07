import k.charts.ChartScalable

/// A whole number.
///
/// Arithmetic `+ - * / %` is built in; `/` truncates. Overflow and division
/// by zero panic in debug and test builds. Number extensions such as
/// `10.minutes` and `24.usd` are members of `Int` and `Double`.
@Native
public struct Int implements Equatable, Hashable, Comparable, Codable, ConfigValue, ChartScalable {
    /// Parses a decimal integer such as `"-42"`.
    ///
    /// - Returns: the number, or `nil` if `text` isn't an integer.
    static func parse(_ text: String) -> Int?

    /// Formats the number in the locale in effect, as `style` says
    /// ([`NumberStyle`]).
    func formatted(as style: NumberStyle) -> String

    /// The duration of this many milliseconds.
    var milliseconds: Duration
    /// The duration of this many seconds.
    var seconds: Duration
    /// The duration of this many seconds; an alias of [`seconds`] that
    /// reads well as `1.second`.
    var second: Duration
    /// The duration of this many minutes.
    var minutes: Duration
    /// The duration of this many minutes; an alias of [`minutes`] for
    /// `1.minute`.
    var minute: Duration
    /// The duration of this many hours.
    var hours: Duration
    /// The duration of this many hours; an alias of [`hours`] for
    /// `1.hour`.
    var hour: Duration
    /// The duration of this many days.
    var days: Duration
    /// The duration of this many days; an alias of [`days`] for `1.day`.
    var day: Duration
    /// The duration of this many weeks.
    var weeks: Duration
    /// The duration of this many weeks; an alias of [`weeks`] for
    /// `1.week`.
    var week: Duration
    /// This many bytes, as a byte count.
    var bytes: Int
    /// This many kilobytes (1,000 bytes each), as a byte count.
    var kilobytes: Int
    /// This many megabytes (1,000,000 bytes each), as a byte count.
    var megabytes: Int
}
