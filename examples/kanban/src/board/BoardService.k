import k.crypto.Uuid
import k.drag.ListMove
import k.storage.LocalStorage
import k.storage.StorageKey
import k.time.Clock

/// Owns every board: the single source of truth the pages read.
///
/// Components read `boards` and call the methods below; only these change
/// the boards. Each change is saved to local storage at once, so a reload
/// shows the latest state, and a change of a board's done or total count
/// adds a point to its progress history, stamped by the injected `Clock`.
///
/// Storage failures never stop the app: the change still applies in
/// memory, and `problem` says what went wrong until the next successful
/// save.
public service BoardService {
    @Inject let storage: LocalStorage
    @Inject let clock: Clock

    /// The longest board or column title, in characters after trimming.
    let maximumTitleLength: Int = 60

    // The boards live under this key. Its name and the shape of `Board`
    // are recorded in storage.lock, so `k check` fails if a change would
    // make every saved board unreadable.
    private let savedBoards = StorageKey<List<Board>>(name: "boards")

    /// Every board, oldest first.
    @State var boards: List<Board> = []

    /// The latest storage failure, or `nil` while storage works.
    @State var problem: StorageProblem? = nil

    /// Restores the saved boards as soon as the service exists.
    init() {
        restore()
    }

    /// The board with `id`, or `nil` when there is none (it was deleted, or
    /// the URL names a board this browser never had).
    func board(id: BoardId) -> Board? {
        return boards.firstWhere { board => board.id == id }
    }

    /// Creates a board with the columns To do, Doing and Done, and saves.
    ///
    /// - Parameter title: the name; trimmed, then checked.
    /// - Returns: the new board's id.
    /// - Throws: `TitleError.empty` or `TitleError.tooLong`; nothing is
    ///   created then.
    func createBoard(title: String) throws TitleError -> BoardId {
        let name = try checked(title: title)
        let id = BoardId(value: Uuid.random())
        let columns = ["To do", "Doing", "Done"].map { t => Column(id: ColumnId(value: Uuid.random()), title: t, cards: []) }

        boards.append(Board(id: id, title: name, columns: columns, history: [ProgressPoint(at: clock.now, done: 0, total: 0)]))
        save()
        return id
    }

    /// Renames a board, and saves. Does nothing for an unknown board.
    ///
    /// - Throws: `TitleError`, as `createBoard(title:)` does.
    func renameBoard(id: BoardId, title: String) throws TitleError {
        let name = try checked(title: title)

        guard let b = indexOfBoard(id: id) else {
            return
        }

        boards[b].title = name
        save()
    }

    /// Deletes a board, and saves.
    func deleteBoard(id: BoardId) {
        boards.removeWhere { board => board.id == id }
        save()
    }

    /// Adds a column to the right of a board's columns, and saves. The new
    /// column becomes the done column.
    ///
    /// - Throws: `TitleError`, as `createBoard(title:)` does.
    func addColumn(board: BoardId, title: String) throws TitleError {
        let name = try checked(title: title)

        guard let b = indexOfBoard(id: board) else {
            return
        }

        boards[b].columns.append(Column(id: ColumnId(value: Uuid.random()), title: name, cards: []))
        changed(board: b)
    }

    /// Adds a card at the bottom of a column, and saves.
    ///
    /// The draft comes from the card editor's `@Form`, which validated it,
    /// so only the title is trimmed here.
    ///
    /// - Returns: the new card's id, or `nil` for an unknown board or column.
    func addCard(board: BoardId, column: ColumnId, draft: CardDraft) -> CardId? {
        guard let b = indexOfBoard(id: board) else {
            return nil
        }

        guard let c = boards[b].columns.indexWhere({ col => col.id == column }) else {
            return nil
        }

        let id = CardId(value: Uuid.random())
        boards[b].columns[c].cards.append(Card(id: id, title: draft.title.trimmed(), notes: draft.notes, priority: .normal))
        changed(board: b)
        return id
    }

    /// Changes a card's title, notes and priority, and saves. Does nothing
    /// for an unknown board or card.
    func updateCard(board: BoardId, card: CardId, draft: CardDraft, priority: Priority) {
        guard let b = indexOfBoard(id: board) else {
            return
        }

        for c in 0..<boards[b].columns.count {
            if let i = boards[b].columns[c].cards.indexWhere({ x => x.id == card }) {
                boards[b].columns[c].cards[i].title = draft.title.trimmed()
                boards[b].columns[c].cards[i].notes = draft.notes
                boards[b].columns[c].cards[i].priority = priority
                save()
                return
            }
        }
    }

    /// Removes a card, and saves.
    func removeCard(board: BoardId, card: CardId) {
        guard let b = indexOfBoard(id: board) else {
            return
        }

        for c in 0..<boards[b].columns.count {
            boards[b].columns[c].cards.removeWhere { x => x.id == card }
        }
        changed(board: b)
    }

    /// Applies a drop on a column's list: removes the card from where it
    /// was, inserts it where it was dropped, and saves.
    ///
    /// A move that no longer matches the board (a column that is gone, a
    /// position past the end, another card at that position) is ignored,
    /// so a stale drop can never duplicate or lose a card.
    func moveCard(board: BoardId, move: ListMove<Card>) {
        guard let b = indexOfBoard(id: board) else {
            return
        }

        guard let from = indexOfColumn(board: b, text: move.fromList) else {
            return
        }

        guard let to = indexOfColumn(board: b, text: move.toList) else {
            return
        }

        guard move.fromIndex >= 0 and move.fromIndex < boards[b].columns[from].cards.count else {
            return
        }

        guard boards[b].columns[from].cards[move.fromIndex].id == move.item.id else {
            return
        }

        let card = boards[b].columns[from].cards.remove(at: move.fromIndex)
        var at = move.toIndex
        if at < 0 {
            at = 0
        }
        if at > boards[b].columns[to].cards.count {
            at = boards[b].columns[to].cards.count
        }
        boards[b].columns[to].cards.insert(card, at: at)
        changed(board: b)
    }

    /// Adds `board`, or replaces the board with its id, and saves: an
    /// import.
    ///
    /// - Returns: the board's id.
    func importBoard(_ board: Board) -> BoardId {
        if let b = indexOfBoard(id: board.id) {
            boards[b] = board
        } else {
            boards.append(board)
        }
        save()
        return board.id
    }

    /// Hides the current storage problem.
    func dismissProblem() {
        problem = nil
    }

    /// Replaces the boards with the saved ones.
    func restore() {
        do {
            boards = try storage.read(key: savedBoards) ?? []
            problem = nil
        } catch .unreadable {
            boards = []
            problem = .unreadable
        } catch .unavailable {
            boards = []
            problem = .unavailable
        }
    }

    /// Records progress when board `b`'s counts changed, then saves.
    private func changed(board b: Int) {
        let done = boards[b].doneCount
        let total = boards[b].cardCount
        let last = boards[b].history.last

        if last == nil or last?.done != done or last?.total != total {
            boards[b].history.append(ProgressPoint(at: clock.now, done: done, total: total))
        }
        save()
    }

    /// Writes every board to storage, and records the outcome.
    private func save() {
        do {
            try storage.write(key: savedBoards, value: boards)
            problem = nil
        } catch .quotaExceeded {
            problem = .full
        } catch .unavailable {
            problem = .unavailable
        }
    }

    /// Trims `title` and checks it.
    private func checked(title: String) throws TitleError -> String {
        let trimmed = title.trimmed()

        guard trimmed.isNotEmpty else {
            throw TitleError.empty
        }

        guard trimmed.count <= maximumTitleLength else {
            throw TitleError.tooLong(limit: maximumTitleLength)
        }

        return trimmed
    }

    /// The position of the board with `id`.
    private func indexOfBoard(id: BoardId) -> Int? {
        return boards.indexWhere { board => board.id == id }
    }

    /// The position of the column whose id text is `text` on board `b`.
    private func indexOfColumn(board b: Int, text: String) -> Int? {
        return boards[b].columns.indexWhere { column => column.id.text == text }
    }
}
