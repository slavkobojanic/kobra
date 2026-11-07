/// Creates `ledger_entry` for `ledger.LedgerEntry`.
migration CreateLedger {
    func up(schema: Schema) {
        schema.createTable(name: "ledger_entry") { table =>
            table.column(name: "id", type: .integer).primaryKey(true)
            table.column(name: "amount", type: .integer)
            table.column(name: "memo", type: .text)
        }
    }
}
