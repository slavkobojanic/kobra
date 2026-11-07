import k.device.File
import k.device.FileType
import k.encoding.Json
import k.test.TestFiles

test BoardTransfer {
    @Inject let files: TestFiles
    @Inject let transfer: BoardTransfer
    @Inject let service: BoardService

    test "an export downloads the board as JSON named after it" {
        let id = try Board.seed(in: service, cards: ["Write tests"])
        try service.renameBoard(id: id, title: "Launch Day")

        transfer.export(board: id)

        expect files.saved.count == 1
        expect files.saved[0].name == "launch-day.json"
        expect files.saved[0].mediaType == "application/json"
        let file: BoardFile = try Json.decode(files.saved[0].bytes)
        expect file.format == 1
        expect file.board.title == "Launch Day"
        expect file.board.columns[0].cards[0].title == "Write tests"
        expect transfer.problem == nil
    }

    test "an exported board imports back, replacing the board with its id" {
        let id = try Board.seed(in: service, cards: ["Write tests"])
        transfer.export(board: id)
        try service.renameBoard(id: id, title: "Renamed since")
        files.choose(files.saved[0])

        let imported = await transfer.importBoard()

        expect imported == id
        expect service.boards.count == 1
        expect service.board(id: id)?.title == "Seed"
        expect files.requests == [[FileType.json]]
    }

    test "a board from another browser is added" {
        files.choose(File.fromText(name: "board.json", type: .json, text: Json.encode(BoardFile(format: 1, board: Board.sample))))

        let imported = await transfer.importBoard()

        expect imported == Board.sample.id
        expect service.boards.count == 1
    }

    test "closing the picker imports nothing and says nothing" {
        let imported = await transfer.importBoard()

        expect imported == nil
        expect transfer.problem == nil
    }

    test "a file that isn't a board is reported" {
        files.choose(File.fromText(name: "notes.json", type: .json, text: Json.encode(CardDraft(title: "Not a board", notes: ""))))

        let imported = await transfer.importBoard()

        expect imported == nil
        expect service.boards.isEmpty
        expect transfer.problem?.message.startsWith("That file isn't a Kanban board") == true
    }

    test "a board of a newer format is refused" {
        files.choose(File.fromText(name: "board.json", type: .json, text: Json.encode(BoardFile(format: 2, board: Board.sample))))

        let imported = await transfer.importBoard()

        expect imported == nil
        expect transfer.problem == .newerFormat(version: 2)
    }

    test "a file too large or unreadable is reported" {
        files.failOpen(error: .tooLarge(limit: File.maximumSize))

        let first = await transfer.importBoard()

        expect first == nil
        expect transfer.problem == .tooLarge(limit: 16777216)

        files.failOpen(error: .unreadable)

        let second = await transfer.importBoard()

        expect second == nil
        expect transfer.problem == .unreadable
    }

    test "a blocked download is reported" {
        let id = try Board.seed(in: service, cards: [])
        files.failSaves(error: .unavailable)

        transfer.export(board: id)

        expect transfer.problem == .downloadsBlocked
        expect files.saved.isEmpty
    }
}
