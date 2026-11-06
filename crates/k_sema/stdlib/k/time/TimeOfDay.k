/// A time of day such as `TimeOfDay("03:00")`, used by job schedules.
@Native
public struct TimeOfDay implements Equatable, Hashable, Codable {
    /// A literal `"HH:MM"`, validated at compile time.
    @Native
    @Literal
    init(_ literal: String) {}

    /// The hour, `0...23`.
    var hour: Int
    /// The minute, `0...59`.
    var minute: Int
}
