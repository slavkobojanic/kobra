/// Creates `note`.
migration CreateNotes {
    func up(schema: Schema) {
        schema.createTable(name: "note") { table =>
            table.column(name: "id", type: .uuid).primaryKey(true)
            table.column(name: "title", type: .text)
            table.column(name: "status", type: .enumeration(cases: ["draft", "published"]))
        }
    }
}
