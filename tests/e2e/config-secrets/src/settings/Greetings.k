import k.rpc.RpcOnlyError

/// The client-side service pages use to reach `GreetingProtocol`.
public service Greetings {
    @Inject let api: GreetingProtocol

    /// The greeting, from the server.
    func text() async throws RpcOnlyError -> String {
        return try await api.greeting()
    }
}
