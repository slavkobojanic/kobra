enum SaveError implements Error {
    case conflict
}

func transaction<T, E>(block: () async throws E -> T) async throws E -> T {
    return try await block()
}

func save() async throws SaveError {}

func run() async throws SaveError -> Int {
    let n = await transaction { 1 }
    try await transaction { try await save() }
    await transaction { try await save() }
    return n
}
