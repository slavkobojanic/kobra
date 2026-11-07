/// A collection of keys and values with value semantics. `[k: v]` is a map
/// literal and `[:]` an empty one. `map[key]` reads an optional value.
@Native
public struct Map<K, V> implements Codable where K implements Hashable {
    /// The number of entries.
    var count: Int
    /// `true` if the map has no entries.
    var isEmpty: Bool
    /// The keys, in insertion order.
    var keys: List<K>
    /// The values, in insertion order.
    var values: List<V>

    /// Returns the value for `key`, or `nil` if the map has none; the same
    /// as `map[key]`.
    func get(_ key: K) -> V?
    /// Sets the value for `key`, replacing any previous one; a new key goes
    /// last in insertion order. Requires a `var` map.
    func set(_ key: K, _ value: V)
    /// Removes the entry for `key`, if any. Requires a `var` map.
    func remove(_ key: K)
    /// Returns `true` if the map has an entry for `key`.
    func containsKey(_ key: K) -> Bool
}
