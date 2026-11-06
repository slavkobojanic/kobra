// A `where` clause lists several predicates, each with its own `implements`.
struct Index<K, V> where K implements Hashable, V implements Codable<K> {}
