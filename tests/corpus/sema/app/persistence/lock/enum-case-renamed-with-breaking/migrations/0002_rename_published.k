/// Renames `published` to `live`, rewriting the rows.
@Breaking("every client is updated together")
migration RenamePublished {
    func up(schema: Schema) {
        schema.alterTable(name: "note") { table =>
            table.renameEnumCase(column: "status", from: "published", to: "live")
        }
    }
}
