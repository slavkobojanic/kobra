/// Notes can be archived.
migration AddArchived {
    func up(schema: Schema) {
        schema.alterTable(name: "note") { table =>
            table.addEnumCase(column: "status", case: "archived")
        }
    }
}
