/// An ordered collection with value semantics: immutable when declared
/// `let`, mutable when declared `var` (`docs/design/08-types.md`).
///
/// `[a, b]` is a list literal and `[]` an empty one. Inside a repository,
/// the entity collection supports the same calls and they are translated to
/// SQL (`docs/design/19-persistence.md`).
@Native
public struct List<T> implements Codable {
    /// The number of elements.
    var count: Int
    /// `true` if the list has no elements.
    var isEmpty: Bool
    /// `true` if the list has at least one element.
    var isNotEmpty: Bool
    /// The first element, or `nil` if the list is empty.
    var first: T?
    /// The last element, or `nil` if the list is empty.
    var last: T?

    /// The elements for which `predicate` is `true`.
    func filter(_ predicate: (T) -> Bool) -> List<T>
    /// The results of `transform` for each element, in order.
    func map<U>(_ transform: (T) -> U) -> List<U>
    /// The non-`nil` results of `transform`.
    func compactMap<U>(_ transform: (T) -> U?) -> List<U>
    /// The concatenated results of `transform`.
    func flatMap<U>(_ transform: (T) -> List<U>) -> List<U>
    /// The elements sorted by `key`, ascending; stable.
    func sortedBy<K>(_ key: (T) -> K) -> List<T> where K implements Comparable
    /// The elements sorted by `key`, descending; stable.
    func sortedByDescending<K>(_ key: (T) -> K) -> List<T> where K implements Comparable
    /// Sorts elements that compare equal by the previous sort by `key`,
    /// ascending; follows `sortedBy` or `sortedByDescending`.
    func thenBy<K>(_ key: (T) -> K) -> List<T> where K implements Comparable
    /// Sorts elements that compare equal by the previous sort by `key`,
    /// descending.
    func thenByDescending<K>(_ key: (T) -> K) -> List<T> where K implements Comparable
    /// The elements in ascending order.
    func sorted() -> List<T> where T implements Comparable
    /// The elements without repeats, first occurrences kept, in order.
    func distinct() -> List<T> where T implements Hashable
    /// The index of the first element for which `predicate` is `true`.
    func indexWhere(_ predicate: (T) -> Bool) -> Int?
    /// The first element for which `predicate` is `true`.
    func firstWhere(_ predicate: (T) -> Bool) -> T?
    /// `true` if `predicate` is `true` for some element.
    func any(_ predicate: (T) -> Bool) -> Bool
    /// `true` if `predicate` is `true` for every element.
    func all(_ predicate: (T) -> Bool) -> Bool
    /// The number of elements for which `predicate` is `true`.
    func count(where predicate: (T) -> Bool) -> Int
    /// The elements grouped by `key`.
    func groupedBy<K>(_ key: (T) -> K) -> Map<K, List<T>> where K implements Hashable
    /// The elements split into those that match `predicate` and the rest.
    func partitioned(_ predicate: (T) -> Bool) -> (List<T>, List<T>)
    /// Consecutive chunks of `size` elements; the last may be shorter.
    func chunked(_ size: Int) -> List<List<T>>
    /// Every run of `size` consecutive elements.
    func windowed(_ size: Int) -> List<List<T>>
    /// The first `count` elements.
    func prefix(_ count: Int) -> List<T>
    /// The element with the smallest `key`, or `nil` if the list is empty.
    func minBy<K>(_ key: (T) -> K) -> T? where K implements Comparable
    /// The element with the largest `key`, or `nil` if the list is empty.
    func maxBy<K>(_ key: (T) -> K) -> T? where K implements Comparable
    /// The sum of `amount` over the elements, in `currency`; `Money.zero` for
    /// an empty list (`docs/design/34-money.md`).
    func sum(in currency: Currency, _ amount: (T) -> Money) -> Money
    /// The elements as text, separated by `separator`.
    func joined(separator: String) -> String
    /// `true` if `element` is in the list.
    func contains(_ element: T) -> Bool where T implements Equatable
    /// `true` if some element satisfies `predicate`; `contains { … }` reads
    /// like `any`.
    func contains(where predicate: (T) -> Bool) -> Bool
    /// The list followed by `other`.
    func appending(_ other: T) -> List<T>
    /// The pairs of each element and its index.
    func enumerated() -> List<(Int, T)>

    /// Appends `element`. Requires a `var` list.
    func append(_ element: T)
    /// Inserts `element` at `index`. Requires a `var` list.
    func insert(_ element: T, at index: Int)
    /// Removes the elements for which `predicate` is `true`. Requires a
    /// `var` list.
    func removeWhere(_ predicate: (T) -> Bool)
    /// Removes the element at `index` and returns it; panics when `index`
    /// is out of range. Requires a `var` list.
    func remove(at index: Int) -> T
    /// Removes every element. Requires a `var` list.
    func removeAll()
}
