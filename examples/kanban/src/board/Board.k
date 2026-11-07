/// A board: columns of cards, and a history of its progress.
///
/// The last column counts as done: a card that reaches it is finished, and
/// the progress chart counts it.
public struct Board implements Codable, Equatable {
    /// Identifies the board. Never changes, even across an export and an
    /// import.
    let id: BoardId

    /// The board's name.
    var title: String

    /// The columns, left to right.
    var columns: List<Column>

    /// The progress after each change that moved a card into or out of
    /// the last column, or added or removed one, oldest first.
    var history: List<ProgressPoint>

    /// Every card of the board.
    var cardCount: Int {
        return columns.map { column => column.cards.count }.sum()
    }

    /// The cards in the last column.
    var doneCount: Int {
        return columns.last?.cards.count ?? 0
    }
}
