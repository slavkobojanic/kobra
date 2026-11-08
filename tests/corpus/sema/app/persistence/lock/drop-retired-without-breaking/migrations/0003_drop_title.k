/// Drops the retired title.
migration DropTitle {
    func up(schema: Schema) {
        schema.alterTable(name: "note") { table =>
            table.dropColumn(name: "title")
        }
    }
}
