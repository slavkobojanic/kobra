import k.crypto.Uuid

/// Identifies a column within its board.
public struct ColumnId implements Codable, Equatable, Hashable {
    /// The random UUID behind this id.
    let value: Uuid

    /// The id as text: the `id` of the column's reorderable list.
    var text: String {
        return "{value}"
    }
}
