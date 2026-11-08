/// Drops the retired title.
@Breaking("drops what earlier migrations retired")
migration DropTitle {
    func up(schema: Schema) {
        schema.alterTable(name: "note") { table =>
            table.dropColumn(name: "title")
        }
    }
}
