/// Adds the reference table `category` with its rows, and renames
/// `purchase.buyer`. A rename never comes from a diff; the lock maps
/// `Purchase.customer` to the new name.
@Breaking("single instance")
migration SeedAndRename {
    func up(schema: Schema) {
        schema.createTable(name: "category") { table =>
            table.column(name: "id", type: .uuid).primaryKey(true)
            table.column(name: "name", type: .text).unique(true)
            table.column(name: "rank", type: .integer)
            table.column(name: "featured", type: .boolean).defaultValue(.boolean(false))
        }

        schema.insert(into: "category", rows: [
            ["id": .uuid(Uuid("00000000-0000-4000-8000-0000000000f1")), "name": .text("Mugs"), "rank": .integer(2)],
            ["id": .uuid(Uuid("00000000-0000-4000-8000-0000000000f2")), "name": .text("Pens & 'quills'"), "rank": .integer(1), "featured": .boolean(true)],
        ])

        schema.alterTable(name: "purchase") { table =>
            table.renameColumn(from: "buyer", to: "buyer_id")
        }
    }
}
