/// Creates `product` and `purchase`.
migration CreateCatalog {
    func up(schema: Schema) {
        schema.createTable(name: "product") { table =>
            table.column(name: "id", type: .uuid).primaryKey(true)
            table.column(name: "slug", type: .text).unique(true)
            table.column(name: "title", type: .text)
            table.column(name: "price", type: .money)
            table.column(name: "weight", type: .decimal)
            table.column(name: "stock", type: .integer)
            table.column(name: "status", type: .enumeration(cases: ["draft", "live", "retired"]))
                .defaultValue(.text("draft"))
            table.column(name: "note", type: .text).nullable(true)
        }

        schema.createTable(name: "purchase") { table =>
            table.column(name: "id", type: .uuid).primaryKey(true)
            table.column(name: "product_id", type: .uuid).references(table: "product")
            table.column(name: "buyer", type: .uuid)
            table.column(name: "placed_at", type: .timestamp).indexed(true)
            table.column(name: "paid_at", type: .timestamp).nullable(true)
        }
    }
}
