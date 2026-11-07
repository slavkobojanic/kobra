// A class is built by its declared `init`; methods update its fields.
class Cache {
    let capacity: Int
    var keys: List<String> = []

    init(capacity: Int) {
        self.capacity = capacity
    }

    func put(key: String) {
        if keys.count == capacity {
            keys.remove(at: 0)
        }
        keys.append(key)
    }
}

func main() -> List<String> {
    let cache = Cache(capacity: 2)
    cache.put(key: "a")
    cache.put(key: "b")
    cache.put(key: "c")
    return cache.keys
}
