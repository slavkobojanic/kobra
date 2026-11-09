/// Where the device is (`docs/design/45-web-platform.md`, "Geolocation"):
/// plain numbers with their unit in the name.
public struct Position implements Equatable, Codable {
    /// Degrees north of the equator (negative south), WGS 84.
    let latitude: Double
    /// Degrees east of Greenwich (negative west), WGS 84.
    let longitude: Double
    /// The radius of 95% confidence around the position, in metres.
    let accuracyMeters: Double
    /// Metres above the WGS 84 ellipsoid, when the device knows it.
    let altitudeMeters: Double?
    /// The direction of travel, clockwise from true north; `nil` when not
    /// moving.
    let headingDegrees: Double?
    /// The ground speed, when the device knows it.
    let speedMetersPerSecond: Double?
    /// When the position was measured.
    let measuredAt: Date

    /// A position for tests: 1 km accuracy, nothing else known, measured
    /// at the [`k.test.TestClock`]'s time. It exists only in `k test`.
    @Native
    static func sample(latitude: Double, longitude: Double) -> Position
}
