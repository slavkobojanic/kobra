# Importing and exporting files

Opening a file the user chooses and saving a file your app makes are the **client-only `Files`
interface in `k.device`**. You inject it into a service, like `LocalStorage`. The design is in
`docs/design/42-files.md`.

```k
// examples/kanban/src/board/BoardTransfer.k
public service BoardTransfer {
    @Inject let files: Files
    @Inject let boards: BoardService
    // ...
    let format: Int = 1
    // ...
    @State var problem: TransferProblem? = nil
    // ...
    func export(board id: BoardId) {
        guard let board = boards.board(id: id) else {
            return
        }

        let name = board.title.lowercased().split(separator: " ").filter { w => w.isNotEmpty }.joined(separator: "-")
        let file = File.fromText(name: "{name}.json", type: .json, text: Json.encode(BoardFile(format: format, board: board)))

        do {
            try files.save(file)
            problem = nil
        } catch .unavailable {
            problem = .downloadsBlocked
        }
    }
    // ...
    func importBoard() async -> BoardId? {
        do {
            let file = try await files.open(accepting: [.json])
            return read(file: file)
        } catch .cancelled {
            return nil
        } catch .tooLarge(let limit) {
            problem = .tooLarge(limit: limit)
            return nil
        } catch .unreadable {
            problem = .unreadable
            return nil
        }
    }
    // ...
    private func read(file: File) -> BoardId? {
        do {
            let decoded: BoardFile = try Json.decode(file.bytes)

            guard decoded.format <= format else {
                problem = .newerFormat(version: decoded.format)
                return nil
            }

            problem = nil
            return boards.importBoard(decoded.board)
        } catch .malformed(let message) {
            problem = .notABoard(detail: message)
            return nil
        } catch .mismatch(let path, let message) {
            problem = .notABoard(detail: "{path}: {message}")
            return nil
        }
    }
}
```

## Opening

- `open(accepting:)` shows the browser's file picker for one file of the given types and resolves once
  it is read; `openAll(accepting:)` allows several. An empty list accepts any file.
- **Call it from the user's action,** before any other `await`: browsers show a picker only in response
  to a click or a key press. `button { "Import…" }.on(click: transfer.importBoard)` does that.
- **The compiler checks this.** `open` and `openAll` are `@UserAction`, and so is every function that
  calls one before its first `await`, like `importBoard`. Calling one from an `@Effect`, a `@Load`, a
  timer or after another `await`, or passing one anywhere but straight to `.on(click: …)`, `.on(submit:
  …)` or `form.handle(…)`, is [K4030](../reference/diagnostics.md#k4030-user-action-required).
- The result is a **`File` value**, read whole: `name`, `mediaType`, `bytes` (`Bytes`), `text` (the
  bytes as UTF-8, or `nil`) and `size`. Decode it into your own types with `Json.decode(file.bytes)`.
- It throws a **`FileOpenError`**: `.cancelled` when the picker is closed, `.tooLarge(limit:)` for a
  file over `File.maximumSize` (16 MiB), and `.unreadable`.

`FileType` names what the picker offers: `.json`, `.text`, `.csv`, `.markdown`, `.png`, `.jpeg`,
`.svg`, `.pdf`, or `.other(mediaType:fileExtension:)`. Each has its `mediaType` and `fileExtension`.

## Saving

`save(_ file:)` starts a download named `file.name`. `File.fromText(name:type:text:)` builds a text file
with the type's media type; a binary file is `File(name:mediaType:bytes:)`. It throws
`FileSaveError.unavailable` when the browser refuses downloads.

## Testing

`k test` puts **`TestFiles`** in place of `Files`: no picker opens and no download starts. Inject it by
its concrete type to say what the user chooses and to read what was saved:

```k
// examples/kanban/src/board/BoardTransfer.test.k
test BoardTransfer {
    @Inject let files: TestFiles
    @Inject let transfer: BoardTransfer
    @Inject let service: BoardService
    // ...
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
    // ...
}
```

`choose(_:)` queues the files the next `open` answers (`open` with nothing chosen is `.cancelled`),
`failOpen(error:)` makes the next one fail, `failSaves(error:)` makes saves fail, and `requests` lists
the types of every `open`.
