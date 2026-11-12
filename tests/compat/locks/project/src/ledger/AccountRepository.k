/// The accounts.
public repository AccountRepository for Account {
    func named(_ name: String) async -> Account? {
        return await accounts.first { account => account.name == name }
    }
}
