// An async throwing call takes both markers, `try await`; the result has the
// function's return type.
enum FetchError implements Error {
    case offline
}

func fetchCount() async throws FetchError -> Int {
    return 3
}

func run() async throws FetchError -> Int {
//   ^^^ type: () async throws corpus.FetchError -> Int
    let count = try await fetchCount()
//      ^^^^^ type: Int
//              ^^^^^^^^^^^^^^^^^^^^^^ type: Int
    return count
}
