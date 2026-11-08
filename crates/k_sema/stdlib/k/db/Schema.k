/// The migration builder passed to `func up(schema: Schema)`
/// (`docs/design/19-persistence.md`, "The Schema builder"). Names tables and
/// columns as strings.
@Native
public class Schema {
    /// Creates the table `name` with the columns `build` adds.
    ///
    /// - Parameters:
    ///   - name: the table name.
    ///   - build: adds the columns with [`TableBuilder.column`].
    func createTable(name: String, _ build: (TableBuilder) -> Void)
    /// Changes the table `name` with the changes `build` makes.
    ///
    /// - Parameters:
    ///   - name: the table name.
    ///   - build: adds, renames and drops columns with [`TableBuilder`].
    func alterTable(name: String, _ build: (TableBuilder) -> Void)
    /// Renames a table; requires `@Breaking("single instance")`, since the
    /// previous release still reads the old name.
    func renameTable(from: String, to: String)
    /// Drops a table; requires [`k.core.Breaking`], and
    /// `@Breaking("single instance")` unless an earlier migration retired
    /// it.
    func dropTable(name: String)
    /// Retires a table: the app stops reading and writing it, and a later
    /// migration drops it.
    func retireTable(name: String)
    /// Adds an index over `columns` of `table`, in order.
    func createIndex(table: String, columns: List<String>)
    /// Inserts data rows into `table`.
    ///
    /// - Parameters:
    ///   - table: the table name.
    ///   - rows: one map from column name to [`ColumnValue`] per row.
    func insert(into table: String, rows: List<Map<String, ColumnValue>>)
    /// Runs raw SQL, written once for each database.
    ///
    /// - Parameters:
    ///   - postgres: the statement run on PostgreSQL.
    ///   - sqlite: the statement run on SQLite.
    func execute(postgres: String, sqlite: String)
}
