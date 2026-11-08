/// Adds a body.
migration AddBody {
    func up(schema: Schema) {
        schema.alterTable(name: "note") { table =>
            table.addColumn(name: "body", type: .text).nullable(true)
        }
    }
}
