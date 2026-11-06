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

func showAll() {
    do {
        try load()
    } catch .offline {
    } catch .notFound {
    }
}
