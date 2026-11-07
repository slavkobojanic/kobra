import board.Board
import board.BoardId
import board.BoardService
import board.Card
import board.CardDraft
import board.CardId
import board.Column
import board.ColumnId
import board.TitleError
import k.crypto.Uuid

// Fixed values for tests that need values, not state. Each `sample` uses
// only the ones above it, which are initialized first.

public extend Card {
    /// A fixed card.
    static let sample: Card = Card(id: CardId(value: Uuid("4b0f7d0e-8a51-4c47-9c3e-5d1f8a2b6c03")), title: "Sample card", notes: "", priority: .normal)
}

public extend ColumnId {
    /// A fixed column id.
    static let sample: ColumnId = ColumnId(value: Uuid("4b0f7d0e-8a51-4c47-9c3e-5d1f8a2b6c02"))
}

public extend Column {
    /// A column holding `Card.sample`.
    static let sample: Column = Column(id: ColumnId.sample, title: "Sample column", cards: [Card.sample])
}

public extend Board {
    /// A board with one column holding `Card.sample`.
    static let sample: Board = Board(id: BoardId(value: Uuid("4b0f7d0e-8a51-4c47-9c3e-5d1f8a2b6c01")), title: "Sample", columns: [Column.sample], history: [])

    /// Creates a board named "Seed" in `boards` with `cards` in its first
    /// column, in order.
    ///
    /// - Returns: the board's id.
    static func seed(in boards: BoardService, cards: List<String>) throws TitleError -> BoardId {
        let id = try boards.createBoard(title: "Seed")
        let first = boards.board(id: id)?.columns[0].id ?? ColumnId.sample

        for title in cards {
            boards.addCard(board: id, column: first, draft: CardDraft(title: title, notes: ""))
        }

        return id
    }
}
