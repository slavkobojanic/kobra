/// A column of a board: a stage cards pass through ("To do", "Doing",
/// "Done"), with its cards in order.
public struct Column implements Codable, Equatable {
    /// Identifies the column. Never changes.
    let id: ColumnId

    /// The column's name, which also names its list for screen readers.
    var title: String

    /// The cards, top first.
    var cards: List<Card>
}
