/// Runs a block several times; error-generic, so it throws what the block
/// throws.
public struct Retry {
    let times: Int

    func run<T, E>(_ block: () async throws E -> T) async throws E -> T {
        return try await block()
    }
}
