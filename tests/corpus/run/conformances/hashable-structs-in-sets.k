// Hashable structs deduplicate in Sets and look up in Maps by value, nested fields included.
struct Name implements Hashable {
    let first: String
    let last: String
}

struct Key implements Hashable {
    let name: Name
    let year: Int
}

func main() -> List<Int> {
    let a = Key(name: Name(first: "A", last: "B"), year: 1)
    let b = Key(name: Name(first: "A", last: "B"), year: 1)
    let c = Key(name: Name(first: "A", last: "C"), year: 1)
    let keys: Set<Key> = Set([a, b, c])
    var counts: Map<Key, Int> = [:]
    counts[a] = 1
    counts[b] = (counts[b] ?? 0) + 1
    return [keys.count, counts.count, counts[a] ?? 0, if keys.contains(Key(name: Name(first: "A", last: "C"), year: 1)) { 1 } else { 0 }]
}
