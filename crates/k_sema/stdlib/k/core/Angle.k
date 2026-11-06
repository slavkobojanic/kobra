/// An angle: `45deg`.
@Native
public struct Angle implements Equatable, Comparable, Codable {
    /// Creates an angle of `degrees`.
    @Native
    init(degrees: Double) {}

    /// The angle in degrees.
    var degrees: Double
}
