/// The values from [`lowerBound`] up to but not including [`upperBound`],
/// written `a..<b`; see also [`ClosedRange`]. A `for` loop over a range of
/// [`Int`] counts.
@Native
public struct HalfOpenRange<T> implements Equatable, Codable where T implements Comparable {
    /// The smallest value in the range.
    var lowerBound: T
    /// The first value after the range.
    var upperBound: T

    /// Returns `true` if `value` is in the range (`upperBound` excluded).
    func contains(_ value: T) -> Bool
    /// Returns the values of the range transformed by `transform`, in
    /// order (for ranges of [`Int`]).
    func map<U>(_ transform: (T) -> U) -> List<U>
}
