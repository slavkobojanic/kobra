/// A time zone, for formatting dates and chart axes.
@Native
public struct TimeZone implements Equatable, Hashable, Codable {
    /// Coordinated Universal Time.
    static let utc: TimeZone
    /// The browser's time zone; on the server, the one the `k_tz` cookie
    /// reports, or UTC on a first visit.
    static let local: TimeZone

    /// Parses an IANA name such as `"Europe/Berlin"`; `nil` if unknown.
    static func parse(_ name: String) -> TimeZone?
    /// The IANA identifier, such as `Europe/Berlin`.
    var identifier: String

    /// The IANA name.
    var name: String
}
