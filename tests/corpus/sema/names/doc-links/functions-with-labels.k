// Functions are linked with their labels, top-level or as members; `()` names one without parameters.

/// Finds one item; see also [`findAll()`] and [`Store.add(item:)`].
func find(id: Int) -> Int {
    return id
}

/// Every item.
func findAll() -> List<Int> {
    return []
}

/// Wraps [`find(id:)`].
func wrapped() -> Int {
    return find(id: 1)
}

/// A store.
struct Store {
    /// Adds [`item`].
    func add(item: Int) -> Int {
        return item
    }
}
