// `do`/`catch` around `try await` in an async function.
enum FetchError implements Error {
    case offline
    case timeout
}

func fetch() async throws FetchError -> Int {
    return 1
}

func show() async -> Int {
    do {
        return try await fetch()
    } catch .offline {
        return 0
    } catch .timeout {
        return -1
    }
}
