/// Serves `LedgerProtocol`.
public service LedgerApi implements LedgerProtocol {
    @Inject let accounts: AccountRepository

    rpc balance(account: String) async throws BalanceError -> Balance {
        guard let found = await accounts.named(account) else {
            throw BalanceError.unknownAccount(name: account)
        }

        return Balance(account: found.name, cents: found.cents, note: nil)
    }
}
