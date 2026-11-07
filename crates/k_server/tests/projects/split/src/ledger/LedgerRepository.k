/// Server-only: a repository, whose queries become SQL.
public repository LedgerRepository for LedgerEntry {
    /// Entries above an amount.
    func large(over: Int) async -> List<LedgerEntry> {
        return await ledgerEntries.filter { entry => entry.amount > over }
    }
}
