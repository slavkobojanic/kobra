// K2206 must carry a note listing the missing patterns and a maybe-incorrect fix
// inserting `catch` clauses for them (diagnostics.md, effects).
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError {}

func show() {
    do {
        try load()
    } catch .offline {
    }
}
