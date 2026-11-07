import k.drag.ListMove
import k.storage.StorageKey
import k.test.MemoryStorage
import k.test.TestClock

test BoardService {
    @Inject let service: BoardService
    @Inject let storage: MemoryStorage
    @Inject let clock: TestClock

    // A test's own key under the service's name, to look at what it stored.
    let savedBoards: StorageKey<List<Board>> = StorageKey(name: "boards")

    /// The board with `id` now, or `Board.sample` when there is none.
    func current(_ id: BoardId) -> Board {
        return service.board(id: id) ?? Board.sample
    }

    test "a new board has To do, Doing and Done, and no cards" {
        let id = try service.createBoard(title: "  Launch  ")
        let board = service.board(id: id)

        expect board?.title == "Launch"
        expect board?.columns.map { column => column.title } == ["To do", "Doing", "Done"]
        expect board?.cardCount == 0
        expect service.boards.count == 1
    }

    test "a blank or too long board name is rejected" {
        expect throws TitleError.empty { try service.createBoard(title: "   ") }
        expect throws TitleError.tooLong(limit: 60) { try service.createBoard(title: "x".repeated(times: 61)) }
        expect service.boards.isEmpty
    }

    test "cards are added to the bottom of a column, trimmed" {
        let id = try service.createBoard(title: "Launch")
        let todo = current(id).columns[0].id

        service.addCard(board: id, column: todo, draft: CardDraft(title: " Write tests ", notes: ""))
        service.addCard(board: id, column: todo, draft: CardDraft(title: "Ship", notes: "Friday"))

        let cards = current(id).columns[0].cards
        expect cards.map { card => card.title } == ["Write tests", "Ship"]
        expect cards[1].notes == "Friday"
        expect cards[0].priority == .normal
    }

    test "a move within a column reorders it" {
        let id = try Board.seed(in: service, cards: ["A", "B", "C"])
        let column = current(id).columns[0]

        service.moveCard(board: id, move: ListMove(item: column.cards[0], fromList: column.id.text, fromIndex: 0, toList: column.id.text, toIndex: 2))

        expect current(id).columns[0].cards.map { card => card.title } == ["B", "C", "A"]
    }

    test "a move between columns takes the card out of one and into the other" {
        let id = try Board.seed(in: service, cards: ["A", "B"])
        let board = current(id)
        let card = board.columns[0].cards[1]

        service.moveCard(board: id, move: ListMove(item: card, fromList: board.columns[0].id.text, fromIndex: 1, toList: board.columns[2].id.text, toIndex: 0))

        let after = current(id)
        expect after.columns[0].cards.map { c => c.title } == ["A"]
        expect after.columns[2].cards.map { c => c.title } == ["B"]
        expect after.doneCount == 1
    }

    test "a move that no longer matches the board is ignored" {
        let id = try Board.seed(in: service, cards: ["A"])
        let board = current(id)

        service.moveCard(board: id, move: ListMove(item: board.columns[0].cards[0], fromList: "gone", fromIndex: 0, toList: board.columns[1].id.text, toIndex: 0))
        service.moveCard(board: id, move: ListMove(item: board.columns[0].cards[0], fromList: board.columns[0].id.text, fromIndex: 5, toList: board.columns[1].id.text, toIndex: 0))

        expect current(id).columns[0].cards.count == 1
        expect current(id).columns[1].cards.isEmpty
    }

    test "editing a card changes its title, notes and priority" {
        let id = try Board.seed(in: service, cards: ["A"])
        let card = current(id).columns[0].cards[0]

        service.updateCard(board: id, card: card.id, draft: CardDraft(title: "A, revised ", notes: "More"), priority: .high)

        let edited = current(id).columns[0].cards[0]
        expect edited.title == "A, revised"
        expect edited.notes == "More"
        expect edited.priority == .high
    }

    test "removing a card and a board" {
        let id = try Board.seed(in: service, cards: ["A", "B"])
        let card = current(id).columns[0].cards[0]

        service.removeCard(board: id, card: card.id)

        expect current(id).cardCount == 1

        service.deleteBoard(id: id)

        expect service.board(id: id) == nil
    }

    test "progress is recorded when the done or total count changes, on the clock" {
        let id = try Board.seed(in: service, cards: ["A"])
        let board = current(id)
        clock.advance(by: 2.hours)

        service.moveCard(board: id, move: ListMove(item: board.columns[0].cards[0], fromList: board.columns[0].id.text, fromIndex: 0, toList: board.columns[2].id.text, toIndex: 0))

        let history = current(id).history
        expect history.map { point => point.done } == [0, 0, 1]
        expect history.map { point => point.total } == [0, 1, 1]
        expect history[2].at == history[1].at + 2.hours
    }

    test "a reorder within a column records no progress" {
        let id = try Board.seed(in: service, cards: ["A", "B"])
        let column = current(id).columns[0]
        let before = current(id).history.count

        service.moveCard(board: id, move: ListMove(item: column.cards[0], fromList: column.id.text, fromIndex: 0, toList: column.id.text, toIndex: 1))

        expect current(id).history.count == before
    }

    test "every change is saved, and a new service restores it" {
        let id = try Board.seed(in: service, cards: ["A"])

        let saved: List<Board> = try storage.read(key: savedBoards) ?? []

        expect saved.count == 1
        expect saved[0].id == id
        expect saved[0].columns[0].cards[0].title == "A"
    }

    test "importing a board adds it, or replaces the board with its id" {
        let id = try Board.seed(in: service, cards: ["A"])
        var copy = current(id)
        copy.title = "Imported"

        let replaced = service.importBoard(copy)

        expect replaced == id
        expect service.boards.count == 1
        expect current(id).title == "Imported"

        let added = service.importBoard(Board.sample)

        expect service.boards.count == 2
        expect added == Board.sample.id
    }

    test "unreadable saved boards start an empty app and say so" {
        storage.writeRaw(name: "boards", text: "{{ not json")

        service.restore()

        expect service.boards.isEmpty
        expect service.problem == .unreadable
    }

    test "a full storage is reported, and the change still applies" {
        storage.failWrites(error: .quotaExceeded)

        let id = try service.createBoard(title: "Launch")

        expect service.problem == .full
        expect service.board(id: id) != nil

        storage.stopFailing()
        try service.renameBoard(id: id, title: "Launch day")

        expect service.problem == nil
    }
}
