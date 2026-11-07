/// Creates `entry`, where jobs record their runs.
migration CreateEntries {
    func up(schema: Schema) {
        schema.createTable(name: "entry") { table =>
            table.column(name: "id", type: .uuid).primaryKey(true)
            table.column(name: "text", type: .text)
        }
    }
}
