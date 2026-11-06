/// The values from `lowerBound` through `upperBound`, written `a...b`.
@Native
public struct ClosedRange<T> implements Equatable, Codable where T implements Comparable {
    /// The smallest value in the range.
    var lowerBound: T
    /// The largest value in the range.
    var upperBound: T

    /// `true` if `value` is in the range.
    func contains(_ value: T) -> Bool
    /// The values of the range transformed by `transform`, in order (for
    /// ranges of `Int`).
    func map<U>(_ transform: (T) -> U) -> List<U>
}
