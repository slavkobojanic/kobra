/// Server-only: injects a config and a repository.
public service LedgerService {
    @Inject let secrets: LedgerSecrets
    @Inject let entries: LedgerRepository

    /// The summary, computed on the server.
    func summary() async -> LedgerSummary {
        let large = await entries.large(over: 100)
        let marker = "SERVER_ONLY_LITERAL_ledger_31f0"

        return LedgerSummary(title: "{secrets.ledgerName} {marker}", total: large.count)
    }

    /// Checks the token.
    func authorize(candidate: String) -> Bool {
        return secrets.token.matches(candidate)
    }
}
