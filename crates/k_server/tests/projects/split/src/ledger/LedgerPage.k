/// Client-safe: a view page.
public page LedgerPage {
    @Inject let ledger: Ledger

    @Load let summary: LedgerSummary = await ledger.summary()

    view {
        heading(level: 1) { summary.title }

        paragraph { "CLIENT_SAFE_LITERAL_total {summary.total}" }
    }

    loading {
        paragraph { "loading the ledger" }
    }
}
