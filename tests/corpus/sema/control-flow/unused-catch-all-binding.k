enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError {}

func run() throws LoadError {
    do {
        try load()
    } catch .offline {
    } catch let error {
    }
}

func rethrow() throws LoadError {
    do {
        try load()
    } catch .offline {
    } catch let error {
        throw error
    }
}
