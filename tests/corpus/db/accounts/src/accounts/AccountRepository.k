import k.crypto.Uuid

/// Accounts.
public repository AccountRepository for Account {
    /// The number of accounts.
    func total() async -> Int {
        return await accounts.count()
    }

    /// The account with an id.
    func byId(_ id: Uuid) async -> Account? {
        return await accounts.first { account => account.id == id }
    }
}
