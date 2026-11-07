/// Client-safe: the service pages use.
public service Ledger {
    @Inject let api: LedgerProtocol

    /// The summary, or a placeholder offline.
    func summary() async -> LedgerSummary {
        do {
            return try await api.summary()
        } catch _ {
            return LedgerSummary(title: "CLIENT_SAFE_LITERAL_offline", total: 0)
        }
    }
}
