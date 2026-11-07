# Importing and exporting files

Opening a file the user chooses and saving a file your app makes are the **client-only `Files`
interface in `k.device`**. You inject it into a service, like `LocalStorage`. The design is in
[42-files.md](../../../design/42-files.md).

```k
import k.device.File
import k.device.Files
import k.encoding.Json

public service BoardTransfer {
    @Inject let files: Files
    @Inject let boards: BoardService

    @State var problem: TransferProblem? = nil

    func export(board: Board) {
        do {
            try files.save(File.fromText(name: "board.json", type: .json, text: Json.encode(board)))
        } catch .unavailable {
            problem = .downloadsBlocked
        }
    }

    func importBoard() async {
        do {
            let file = try await files.open(accepting: [.json])
            read(file: file)
        } catch .cancelled {
            // The user closed the picker: nothing to say.
        } catch .tooLarge(let limit) {
            problem = .tooLarge(limit: limit)
        } catch .unreadable {
            problem = .unreadable
        }
    }

    // Decoding is the app's step, with its own error.
    private func read(file: File) {
        do {
            let board: Board = try Json.decode(file.bytes)
            boards.add(board)
        } catch let decoding {
            problem = .notABoard(decoding)
        }
    }
}
```

## Opening

- `open(accepting:)` shows the browser's file picker for one file of the given types and resolves once
  it is read; `openAll(accepting:)` allows several. An empty list accepts any file.
- **Call it from the user's action,** before any other `await`: browsers show a picker only in response
  to a click or a key press. `button { "Import…" }.on(click: transfer.importBoard)` does that.
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
test BoardTransfer {
    @Inject let files: TestFiles
    @Inject let transfer: BoardTransfer

    test "an exported board imports back" {
        transfer.export(board: Board.sample)
        files.choose(files.saved[0])

        await transfer.importBoard()

        expect transfer.problem == nil
    }
}
```

`choose(_:)` queues the files the next `open` answers (`open` with nothing chosen is `.cancelled`),
`failOpen(error:)` makes the next one fail, `failSaves(error:)` makes saves fail, and `requests` lists
the types of every `open`.
