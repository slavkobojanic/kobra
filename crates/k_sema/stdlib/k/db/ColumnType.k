/// The column types of the migration builder ([`TableBuilder.column`]),
/// mapped per database.
public enum ColumnType {
    /// A UUID column.
    case uuid
    /// A text column.
    case text
    /// A whole-number column.
    case integer
    /// An exact decimal column.
    case decimal
    /// A boolean column.
    case boolean
    /// A timestamp column.
    case timestamp
    /// A [`Money`] value: one logical column that the builder expands to
    /// `<name>_amount` and `<name>_currency` for every operation.
    case money
    /// A JSON column.
    case json
    /// A column holding the name of one of `cases`, for an enum field.
    case enumeration(cases: List<String>)
}
