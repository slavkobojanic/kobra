/// Creates `account`.
migration CreateAccounts {
    func up(schema: Schema) {
        schema.createTable(name: "account") { table =>
            table.column(name: "id", type: .uuid).primaryKey(true)
            table.column(name: "name", type: .text).unique(true)
            table.column(name: "cents", type: .integer)
        }
    }
}
