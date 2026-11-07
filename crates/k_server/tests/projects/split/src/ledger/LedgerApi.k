import k.rpc.RpcOnlyError

/// Server-only: the implementation.
public service LedgerApi implements LedgerProtocol {
    @Inject let ledger: LedgerService

    rpc summary() async throws RpcOnlyError -> LedgerSummary {
        let note = implementationNote()

        if note.isEmpty {
            return LedgerSummary(title: "IMPL_LITERAL_4471", total: 0)
        }

        return await ledger.summary()
    }
}

/// Server-only by being used only by the implementation.
func implementationNote() -> String {
    return "HELPER_LITERAL_c0ffee"
}
