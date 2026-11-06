// `E` is inferred from the block, so it propagates like any other error type:
// a `SaveError` can't leave a function that throws `PageError` (K2203).
enum SaveError implements Error {
    case conflict
    case rejected
}

func save() async throws SaveError {}

func transaction<T, E>(_ block: () async throws E -> T) async throws E -> T {
    return try await block()
}

enum PageError implements Error {
    case failed
}

func page() async throws PageError {
    try await transaction { try await save() }
}
