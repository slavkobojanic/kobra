// `try await f() else { … }` works for async throwing calls.
enum FetchError implements Error {
    case offline
}

func fetch() async throws FetchError -> Int {
    return 1
}

func run() async -> Int {
    let value = try await fetch() else { return 0 }
//      ^^^^^ type: Int
    return value
}
