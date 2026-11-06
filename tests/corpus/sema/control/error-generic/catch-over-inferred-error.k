// `catch` over an error-generic call is exhaustive over the inferred `E`; the
// catch-all binding has that type.
enum SaveError implements Error {
    case conflict
    case rejected
}

func save() async throws SaveError {}

func transaction<T, E>(_ block: () async throws E -> T) async throws E -> T {
    return try await block()
}

func run() async -> String {
    do {
        try await transaction { try await save() }
    } catch .conflict {
        return "conflict"
    } catch let error {
//              ^^^^^ type: corpus.SaveError
        return "failed: {error}"
    }
    return "saved"
}

func runPartial() async -> String {
    do {
        try await transaction { try await save() }
    } catch .conflict {
        return "conflict"
    }
    return "saved"
}
