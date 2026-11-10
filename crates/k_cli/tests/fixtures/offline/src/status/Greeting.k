/// Asks the server for its greeting, and says why there is none.
public service Greeting {
    @Inject let status: StatusProtocol

    /// The server's greeting, or what went wrong reaching it.
    func ask() async -> String {
        do {
            return try await status.greeting()
        } catch .rpcFailure(.offline) {
            return "The server can't be reached: you're offline."
        } catch .rpcFailure(.timeout) {
            return "The server took too long to answer."
        } catch .rpcFailure {
            return "The server refused."
        }
    }
}
