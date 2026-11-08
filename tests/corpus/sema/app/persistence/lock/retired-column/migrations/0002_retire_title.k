/// Notes lose their title.
migration RetireTitle {
    func up(schema: Schema) {
        schema.alterTable(name: "note") { table =>
            table.retireColumn(name: "title")
        }
    }
}
