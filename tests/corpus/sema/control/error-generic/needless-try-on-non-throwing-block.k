// `try` on an error-generic call whose block can't throw covers nothing that
// can throw: K2202.
enum SaveError implements Error {
    case conflict
    case rejected
}

func save() async throws SaveError {}

func transaction<T, E>(_ block: () async throws E -> T) async throws E -> T {
    return try await block()
}

func count() async -> Int {
    return try await transaction { 1 }
}
