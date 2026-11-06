// Doc comments before items and members do not change the tree shape.
/// A point on the plane.
///
/// - Note: immutable.
public struct Point {
    /// The horizontal coordinate.
    let x: Int

    /// Moves the point.
    func moved(by delta: Int) -> Point {
        return Point(x: x + delta)
    }
}
