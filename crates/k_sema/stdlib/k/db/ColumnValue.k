import k.crypto.Uuid

/// A value in a migration: a default or a data row's cell.
public enum ColumnValue {
    /// SQL `NULL`.
    case null
    /// Text.
    case text(String)
    /// A whole number.
    case integer(Int)
    /// A boolean.
    case boolean(Bool)
    /// An exact decimal.
    case decimal(Decimal)
    /// A UUID.
    case uuid(Uuid)
}
