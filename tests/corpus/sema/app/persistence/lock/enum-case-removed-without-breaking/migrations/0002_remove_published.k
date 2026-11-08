/// Drops publishing.
migration RemovePublished {
    func up(schema: Schema) {
        schema.alterTable(name: "note") { table =>
            table.removeEnumCase(column: "status", case: "published", replacement: "draft")
        }
    }
}
