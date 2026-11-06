// Error-generic calls nest: the inner call's `E` is the block's, the outer
// call's `E` is the inner call's.
enum SaveError implements Error {
    case conflict
    case rejected
}

func save() async throws SaveError {}

func transaction<T, E>(_ block: () async throws E -> T) async throws E -> T {
    return try await block()
}

func run() async throws SaveError -> Int {
    return try await transaction {
        try await transaction {
            try await save()
            1
        }
    }
}

func count() async -> Int {
    return await transaction {
        await transaction { 1 }
    }
}
