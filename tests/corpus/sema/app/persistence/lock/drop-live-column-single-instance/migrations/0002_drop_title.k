/// Drops the title in one step.
@Breaking("single instance")
migration DropTitle {
    func up(schema: Schema) {
        schema.alterTable(name: "note") { table =>
            table.dropColumn(name: "title")
        }
    }
}
