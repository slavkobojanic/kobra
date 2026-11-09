/// The `account` table, for the server's code.
public repository AccountRepository for Account {
    /// Returns the account with this email address, or `nil` when there is
    /// none.
    func byEmail(email: String) async -> Account? {
        return await accounts.first { account => account.email == email }
    }
}
