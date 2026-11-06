// Only a generic error passes through. A parameter with a concrete error type
// makes the function's own body responsible: an undeclared `try` is K2203.
enum SaveError implements Error {
    case conflict
    case rejected
}

func save() async throws SaveError {}

func transaction<T, E>(_ block: () async throws E -> T) async throws E -> T {
    return try await block()
}

func runBlock(block: () async throws SaveError -> Int) async -> Int {
    return try await block()
}
