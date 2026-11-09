import accounts.AccountId

/// Looks up `Vault`s. A vault is stored with the inherited `insert`.
public repository VaultRepository for Vault {
    /// Returns an account's vault, if it has set one up.
    func forOwner(ownerId: AccountId) async -> Vault? {
        return await vaults.first { vault => vault.ownerId == ownerId }
    }
}
