// A generic class with private state, an initializer and methods.
class LruCache<K, V> {
    private var store: Map<K, V> = [:]
    let capacity: Int

    init(capacity: Int) {
        self.capacity = capacity
    }

    func get(key: K) -> V? {
        return store[key]
    }

    func put(key: K, value: V) {
        store[key] = value
    }
}
