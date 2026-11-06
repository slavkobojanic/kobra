// The `else` block may exit instead of producing a value; the binding then has
// the call's type on the path that continues.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func show() -> Int {
    let value = try load() else { return -1 }
//      ^^^^^ type: Int
    return value
}

func each(items: List<Int>) {
    for item in items {
        let value = try load() else { continue }
        let other = try load() else { break }
    }
}
