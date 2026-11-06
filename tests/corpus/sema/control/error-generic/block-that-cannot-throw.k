// A block that can't throw makes `E` empty, so the call can't throw either and
// needs no `try`; the call has the block's value type.
enum SaveError implements Error {
    case conflict
    case rejected
}

func save() async throws SaveError {}

func transaction<T, E>(_ block: () async throws E -> T) async throws E -> T {
    return try await block()
}

func count() async -> Int {
    let n = await transaction { 1 }
//      ^ type: Int
//          ^^^^^^^^^^^^^^^^^^^^^^^ type: Int
    return n
}
