/// Creates `visit`, which gives the app a database.
migration CreateVisits {
    func up(schema: Schema) {
        schema.createTable(name: "visit") { table =>
            table.column(name: "id", type: .uuid).primaryKey(true)
            table.column(name: "who", type: .text)
        }
    }
}
