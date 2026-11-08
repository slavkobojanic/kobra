/// Creates `charge`, so the app's entities, migrations and schema.lock
/// agree (the case is about environments, not persistence).
migration CreateCharges {
    func up(schema: Schema) {
        schema.createTable(name: "charge") { table =>
            table.column(name: "id", type: .integer).primaryKey(true)
        }
    }
}
