// The errors a load can throw are serialized for `failed` too, so they are `Codable`.
enum FetchError implements Error {
    case offline
}

func fetch() async throws FetchError -> Int {
    return 1
}

page Stats {
    @Load let total: Int = try await fetch()

    view {
        text { "{total}" }
    }

    loading {
    }
}
