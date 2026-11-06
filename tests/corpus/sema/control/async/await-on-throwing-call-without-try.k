// `await` alone on an async throwing call is K2201 (missing `try`), whose span
// includes the `await`; nothing about `await` is reported.
enum FetchError implements Error {
    case offline
}

func fetchCount() async throws FetchError -> Int {
    return 3
}

func run() async throws FetchError -> Int {
    return await fetchCount()
}
