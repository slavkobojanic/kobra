/// The columns of a table being created or changed in a migration.
///
/// A migration with a destructive operation (dropping or renaming a
/// column, renaming or removing an enum case) needs [`k.core.Breaking`];
/// dropping or renaming a column no earlier migration retired needs
/// `@Breaking("single instance")`.
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
    /// Renames a column; requires `@Breaking("single instance")`, since
    /// the previous release still reads the old name.
    func renameColumn(from: String, to: String)
    /// Drops a column; requires [`k.core.Breaking`], and
    /// `@Breaking("single instance")` unless an earlier migration retired
    /// it.
    func dropColumn(name: String)
    /// Retires a column: the app stops reading and writing it, and a
    /// later migration drops it. A `NOT NULL` column without a default
    /// becomes nullable, so the next release's inserts can leave it out.
    func retireColumn(name: String)
    /// Adds the case `name` to the enumeration column `column`.
    func addEnumCase(column: String, case name: String)
    /// Renames an enum case of `column`; requires [`k.core.Breaking`].
    func renameEnumCase(column: String, from: String, to: String)
    /// Removes the case `name` from `column`, rewriting the rows that hold
    /// it to `replacement`; requires [`k.core.Breaking`].
    func removeEnumCase(column: String, case name: String, replacement: String)
}
