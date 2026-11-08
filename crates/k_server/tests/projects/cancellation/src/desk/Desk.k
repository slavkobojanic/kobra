import k.rpc.RpcOnlyError

/// The client-side service the pages call the protocol through.
public service Desk {
    @Inject let api: DeskProtocol

    func save(text: String) async throws RpcOnlyError -> String {
        return try await api.save(text: text)
    }

    func signOut() async throws RpcOnlyError {
        try await api.signOut()
    }

    func lookup(id: Int) async throws RpcOnlyError -> String {
        return try await api.lookup(id: id)
    }
}
