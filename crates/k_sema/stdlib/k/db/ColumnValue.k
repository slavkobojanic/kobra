import k.crypto.Uuid

/// A value in a migration: a default ([`ColumnBuilder.defaultValue`]) or a
/// data row's cell ([`Schema.insert`]).
public enum ColumnValue {
    /// SQL `NULL`.
    case null
    /// A text value.
    case text(String)
    /// A whole number.
    case integer(Int)
    /// A boolean value.
    case boolean(Bool)
    /// An exact decimal.
    case decimal(Decimal)
    /// A UUID value.
    case uuid(Uuid)
}
