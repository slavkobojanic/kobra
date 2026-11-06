// A throwing call nested in an argument is reported on the nested call itself.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func show(value: Int) {}

func run() throws LoadError {
    show(value: load())
}
