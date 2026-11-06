/// The column types of the migration builder, mapped per database.
public enum ColumnType {
    /// A UUID.
    case uuid
    /// Text.
    case text
    /// A whole number.
    case integer
    /// An exact decimal.
    case decimal
    /// A boolean.
    case boolean
    /// A timestamp.
    case timestamp
    /// Money, stored as `<name>_amount` and `<name>_currency`.
    case money
    /// JSON.
    case json
    /// An enum's case names.
    case enumeration(cases: List<String>)
}
