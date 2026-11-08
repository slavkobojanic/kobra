/// Names a table that doesn't exist.
@Breaking("single instance")
migration Typo {
    func up(schema: Schema) {
        schema.alterTable(name: "notes") { table =>
            table.dropColumn(name: "title")
        }
    }
}
