/// Renames the column.
migration RenameTitle {
    func up(schema: Schema) {
        schema.alterTable(name: "note") { table =>
            table.renameColumn(from: "title", to: "heading")
        }
    }
}
