/// Looks up `Account`s. New accounts are stored with the inherited `insert`.
public repository AccountRepository for Account {
    /// Returns the account registered under an email address.
    ///
    /// - Parameter email: an address already trimmed and lower-cased with
    ///   `normalized(email:)`.
    /// - Returns: the account, or `nil` when no account uses the address.
    public func byEmail(email: String) async -> Account? {
        return accounts.first { account => account.email == email }
    }
}
