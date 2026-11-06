// An unresolved error type in `throws` is K1202 only: the `try`s and `catch`
// clauses that involve it are not checked further.
func load() throws Missing -> Int {
    return 1
}

func show() -> Int {
    do {
        return try load()
    } catch .offline {
        return 0
    }
}
