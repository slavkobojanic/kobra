/// Creates `account` with a unique email, and `audit`. A `let` binds a
/// string once for both dialects.
migration CreateAccounts {
    func up(schema: Schema) {
        let audit = "audit"

        schema.createTable(name: "account") { table =>
            table.column(name: "id", type: .uuid).primaryKey(true)
            table.column(name: "email", type: .text).unique(true)
            table.column(name: "handle", type: .text).unique(true)
            table.column(name: "name", type: .text)
        }

        schema.createTable(name: audit) { table =>
            table.column(name: "id", type: .uuid).primaryKey(true)
            table.column(name: "message", type: .text)
        }

        let touch = "UPDATE account SET name = name"
        schema.execute(postgres: touch, sqlite: touch)
    }
}
