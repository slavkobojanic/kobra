/// The options of a column, chained with their values:
/// `.primaryKey(true)`, `.nullable(true)`.
@Native
public class ColumnBuilder {
    /// Makes the column the primary key when `value` is `true`.
    func primaryKey(_ value: Bool) -> ColumnBuilder
    /// Adds a unique index when `value` is `true`.
    func unique(_ value: Bool) -> ColumnBuilder
    /// Allows `NULL` when `value` is `true`.
    func nullable(_ value: Bool) -> ColumnBuilder
    /// Adds an index when `value` is `true`.
    func indexed(_ value: Bool) -> ColumnBuilder
    /// Adds a foreign key to `table`'s primary key; implies an index.
    func references(table: String) -> ColumnBuilder
    /// Sets the value of the column in rows that don't give one.
    func defaultValue(_ value: ColumnValue) -> ColumnBuilder
}
