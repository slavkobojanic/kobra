/// Queries and updates for `MerchantAccount`.
public repository MerchantRepository for MerchantAccount {
    /// Returns the merchant called `name`.
    ///
    /// - Parameter name: the merchant's name, from its service token.
    /// - Returns: the merchant, or `nil` when no merchant of that name has
    ///   registered.
    func byName(_ name: String) async -> MerchantAccount? {
        return await merchantAccounts.first { merchant => merchant.name == name }
    }

    /// Changes where a registered merchant receives its events.
    ///
    /// - Parameters:
    ///   - name: the merchant's name.
    ///   - callbackUrl: the new callback URL.
    /// - Returns: `true` when a merchant of that name exists and was
    ///   updated.
    func moveCallback(name: String, to callbackUrl: Url) async -> Bool {
        let updated = await merchantAccounts
            .filter { merchant => merchant.name == name }
            .update { merchant => merchant.callbackUrl = callbackUrl }

        return updated == 1
    }
}
