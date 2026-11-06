// `for await x in stream { }` reads a stream.
func watch() async throws RpcFailure {
    let changes = try await api.watch()

    for await change in changes {
        apply(change: change)
    }
    for await (id, value) in pairs {
        break
    }
}
