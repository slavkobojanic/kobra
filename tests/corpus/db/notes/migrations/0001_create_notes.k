/// Creates `note` and `tag`.
migration CreateNotes {
    func up(schema: Schema) {
        schema.createTable(name: "note") { table =>
            table.column(name: "id", type: .uuid).primaryKey(true)
            table.column(name: "owner", type: .text).indexed(true)
            table.column(name: "title", type: .text)
            table.column(name: "body", type: .text)
            table.column(name: "revision", type: .integer)
            table.column(name: "created_at", type: .timestamp)
        }

        schema.createTable(name: "tag") { table =>
            table.column(name: "id", type: .uuid).primaryKey(true)
            table.column(name: "note_id", type: .uuid).references(table: "note")
            table.column(name: "label", type: .text)
        }
    }
}
