/// Creates `note`, where posted messages are stored.
migration CreateNotes {
    func up(schema: Schema) {
        schema.createTable(name: "note") { table =>
            table.column(name: "id", type: .uuid).primaryKey(true)
            table.column(name: "text", type: .text)
        }
    }
}
