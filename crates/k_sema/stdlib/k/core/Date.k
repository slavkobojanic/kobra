import k.charts.ChartScalable

/// An instant in time, with millisecond precision
/// (`docs/design/35-time.md`).
///
/// The current instant comes only from an injected `Clock`: there is no
/// `Date.now`. `date + 3.days` and `date - 2.hours` are dates, and
/// `a - b` of two dates is a `Duration`.
@Native
public struct Date implements Equatable, Hashable, Comparable, Codable, ChartScalable {
    /// The instant `seconds` after 1970-01-01T00:00:00Z.
    @Native
    init(unixSeconds: Int) {}

    /// The seconds since 1970-01-01T00:00:00Z, rounded down.
    var unixSeconds: Int

    /// Parses an RFC 3339 timestamp; `nil` if `text` isn't one.
    static func parse(_ text: String) -> Date?

    /// Formats the date in the locale in effect. `.relative` requires
    /// `from:`, the reference instant (usually `clock.now`).
    func formatted(as style: DateStyle, from reference: Date? = nil) -> String
}
