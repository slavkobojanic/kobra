// An unknown member on a known type is K1203 only, with no effect follow-ups.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

struct Store {
    let name: String
}

func show(store: Store) -> Int {
    return try store.reload()
}
