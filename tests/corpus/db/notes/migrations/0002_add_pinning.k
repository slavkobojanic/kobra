/// Adds pinning; existing notes start unpinned.
migration AddPinning {
    func up(schema: Schema) {
        schema.alterTable(name: "note") { table =>
            table.addColumn(name: "is_pinned", type: .boolean)
                .defaultValue(.boolean(false))
        }
    }
}
