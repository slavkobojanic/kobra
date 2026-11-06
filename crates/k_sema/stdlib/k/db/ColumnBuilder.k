/// The options of a column, chained with their values:
/// `.primaryKey(true)`, `.nullable(true)`.
@Native
public class ColumnBuilder {
    /// Makes the column the primary key.
    func primaryKey(_ value: Bool) -> ColumnBuilder
    /// Adds a unique index.
    func unique(_ value: Bool) -> ColumnBuilder
    /// Allows `NULL`.
    func nullable(_ value: Bool) -> ColumnBuilder
    /// Adds an index.
    func indexed(_ value: Bool) -> ColumnBuilder
    /// Adds a foreign key to `table`'s primary key; implies an index.
    func references(table: String) -> ColumnBuilder
    /// Sets a default value.
    func defaultValue(_ value: ColumnValue) -> ColumnBuilder
}
