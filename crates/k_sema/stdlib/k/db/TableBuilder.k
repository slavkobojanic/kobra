/// The columns of a table being created or changed in a migration.
@Native
public class TableBuilder {
    /// Adds a column to a new table.
    func column(name: String, type: ColumnType) -> ColumnBuilder
    /// Adds a column to an existing table; it must be nullable or have a
    /// default.
    func addColumn(name: String, type: ColumnType) -> ColumnBuilder
    /// Renames a column.
    func renameColumn(from: String, to: String)
    /// Drops a column; requires `@Breaking`.
    func dropColumn(name: String)
    /// Adds an enum case to an enumeration column.
    func addEnumCase(column: String, case name: String)
    /// Renames an enum case; requires `@Breaking`.
    func renameEnumCase(column: String, from: String, to: String)
    /// Removes an enum case, rewriting stored values; requires `@Breaking`.
    func removeEnumCase(column: String, case name: String, replacement: String)
}
