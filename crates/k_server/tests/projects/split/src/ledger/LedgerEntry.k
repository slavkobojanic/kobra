/// Server-only: an entity, stored in `ledger_entry`.
public entity LedgerEntry {
    @Id let id: Int
    let amount: Int
    let memo: String
}
