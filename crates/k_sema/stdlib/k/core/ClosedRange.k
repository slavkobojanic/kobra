/// The values from [`lowerBound`] through [`upperBound`], written `a...b`;
/// see also [`HalfOpenRange`].
@Native
public struct ClosedRange<T> implements Equatable, Codable where T implements Comparable {
    /// The smallest value in the range.
    var lowerBound: T
    /// The largest value in the range.
    var upperBound: T

    /// Returns `true` if `value` is in the range, bounds included.
    func contains(_ value: T) -> Bool
    /// Returns the values of the range transformed by `transform`, in
    /// order (for ranges of [`Int`]).
    func map<U>(_ transform: (T) -> U) -> List<U>
}
