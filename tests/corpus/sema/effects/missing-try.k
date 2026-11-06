enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError {}

func run() throws LoadError {
    try load()
    load()
}
