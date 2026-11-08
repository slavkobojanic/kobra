/// Renames `published` to `live`, rewriting the rows.
migration RenamePublished {
    func up(schema: Schema) {
        schema.alterTable(name: "note") { table =>
            table.renameEnumCase(column: "status", from: "published", to: "live")
        }
    }
}
