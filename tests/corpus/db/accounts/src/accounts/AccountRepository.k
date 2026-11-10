import k.crypto.Uuid

/// Accounts.
public repository AccountRepository for Account {
    /// Inserts `account` unless its email or handle is taken: the
    /// collection's insert throws the violation inside the repository too,
    /// so callers needn't catch it.
    ///
    /// - Returns: `false` if one was taken.
    func claim(_ account: Account) async -> Bool {
        do {
            try await accounts.insert(account)
        } catch .email, .handle {
            return false
        }

        return true
    }

    /// Writes `account` unless its handle is taken, with the inherited
    /// `update` named without a receiver.
    ///
    /// - Returns: `false` if it was.
    func renameIfFree(_ account: Account) async -> Bool {
        do {
            try await update(account)
        } catch .handle {
            return false
        } catch .email {
            return false
        }

        return true
    }

    /// The number of accounts.
    func total() async -> Int {
        return await accounts.count()
    }

    /// The account with an id.
    func byId(_ id: Uuid) async -> Account? {
        return await accounts.first { account => account.id == id }
    }
}
