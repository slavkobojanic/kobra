/// An unordered collection of distinct elements with value semantics:
/// `Set(["new", "sale"])`.
@Native
public struct Set<T> implements Codable where T implements Hashable {
    /// Creates a set of the distinct elements of `elements`.
    @Native
    init(_ elements: List<T>) {}

    /// The number of elements.
    var count: Int
    /// `true` if the set has no elements.
    var isEmpty: Bool

    /// `true` if `element` is in the set.
    func contains(_ element: T) -> Bool
    /// Adds `element`. Requires a `var` set.
    func insert(_ element: T)
    /// Removes `element`. Requires a `var` set.
    func remove(_ element: T)
}
