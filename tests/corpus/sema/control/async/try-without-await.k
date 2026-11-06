// `try` alone on an async throwing call: the `await` is missing (K2204 at the
// call, `await ` inserted after `try`), and `try` is not needless.
enum FetchError implements Error {
    case offline
}

func fetchCount() async throws FetchError -> Int {
    return 3
}

func run() async throws FetchError -> Int {
    return try fetchCount()
}
