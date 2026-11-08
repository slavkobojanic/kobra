/// Drops the title in one step.
@Breaking("titles are gone")
migration DropTitle {
    func up(schema: Schema) {
        schema.alterTable(name: "note") { table =>
            table.dropColumn(name: "title")
        }
    }
}
