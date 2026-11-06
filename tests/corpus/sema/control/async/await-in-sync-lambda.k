// A lambda checked against a function type that isn't async is not async, even
// inside an async function: `await` in it is K2205 (List.map takes `(T) -> U`).
func fetchCount(id: Int) async -> Int {
    return id
}

func run(ids: List<Int>) async -> List<Int> {
    return ids.map { id => await fetchCount(id: id) }
}
