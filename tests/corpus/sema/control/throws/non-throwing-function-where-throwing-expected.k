// Function subtyping: a function that throws nothing is accepted where a
// throwing function type is expected (sema.md §9.6). The call through the
// parameter still needs `try`, because its type throws.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func one() -> Int {
    return 1
}

func run(read: () throws LoadError -> Int) throws LoadError -> Int {
    return try read()
}

func main() throws LoadError -> Int {
    return try run(read: one) + run(read: load)
}
