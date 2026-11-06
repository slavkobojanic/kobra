/// The migration builder passed to `func up(schema: Schema)`
/// (`docs/design/19-persistence.md`, "The Schema builder"). Names tables and
/// columns as strings.
@Native
public class Schema {
    /// Creates a table.
    func createTable(name: String, _ build: (TableBuilder) -> Void)
    /// Changes a table.
    func alterTable(name: String, _ build: (TableBuilder) -> Void)
    /// Renames a table.
    func renameTable(from: String, to: String)
    /// Drops a table; requires `@Breaking`.
    func dropTable(name: String)
    /// Adds an index over columns.
    func createIndex(table: String, columns: List<String>)
    /// Inserts data rows.
    func insert(into table: String, rows: List<Map<String, ColumnValue>>)
    /// Runs raw SQL written for both databases.
    func execute(postgres: String, sqlite: String)
}
