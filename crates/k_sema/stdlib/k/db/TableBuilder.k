/// The columns of a table being created or changed in a migration.
///
/// The first compiler releases don't yet enforce the
/// [`k.core.Breaking`] requirement on destructive operations, and refuse
/// the enum case operations (`addEnumCase`, `renameEnumCase`,
/// `removeEnumCase`) when the migrations are replayed, because they need
/// a table rebuild on SQLite; write such a change with
/// [`Schema.execute`] instead.
@Native
public class TableBuilder {
    /// Adds a column to a new table ([`Schema.createTable`]).
    ///
    /// - Returns: the column's options, chained:
    ///   `.column(name: "id", type: .uuid).primaryKey(true)`.
    func column(name: String, type: ColumnType) -> ColumnBuilder
    /// Adds a column to an existing table ([`Schema.alterTable`]); it must
    /// be nullable or have a default.
    ///
    /// - Returns: the column's options, chained.
    func addColumn(name: String, type: ColumnType) -> ColumnBuilder
    /// Renames a column.
    func renameColumn(from: String, to: String)
    /// Drops a column; requires [`k.core.Breaking`].
    func dropColumn(name: String)
    /// Adds the case `name` to the enumeration column `column`.
    func addEnumCase(column: String, case name: String)
    /// Renames an enum case of `column`; requires [`k.core.Breaking`].
    func renameEnumCase(column: String, from: String, to: String)
    /// Removes the case `name` from `column`, rewriting the rows that hold
    /// it to `replacement`; requires [`k.core.Breaking`].
    func removeEnumCase(column: String, case name: String, replacement: String)
}
