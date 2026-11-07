# Files: Import and Export

## Decision

Opening a file the user chooses and saving a file the app generates are the **client-only `Files` platform interface in `k.device`**, injected into a service like `LocalStorage` ([36-local-storage.md](36-local-storage.md)). A chosen file comes back as a **`File` value** with its whole content as `Bytes` and as text; saving starts a download. Both directions fail with **typed errors**.

```k
import k.device.File
import k.device.Files
import k.encoding.Json

public service BoardTransfer {
    @Inject let files: Files
    @Inject let boards: BoardService

    @State var problem: TransferProblem? = nil

    // Asks for a .json file and replaces the board with it.
    func importBoard() async {
        do {
            let file = try await files.open(accepting: [.json])
            read(file: file)
        } catch .cancelled {
            // The user changed their mind: nothing to say.
        } catch .tooLarge(let limit) {
            problem = .tooLarge(limit: limit)
        } catch .unreadable {
            problem = .unreadable
        }
    }

    // Decoding is the app's step, with the app's knowledge of the format.
    private func read(file: File) {
        do {
            let board: Board = try Json.decode(file.bytes)
            boards.replace(with: board)
        } catch let decoding {
            problem = .notABoard(decoding)
        }
    }

    // Saves the board as board.json in the user's downloads.
    func exportBoard() {
        do {
            try files.save(File.fromText(name: "board.json", type: .json, text: Json.encode(boards.board)))
        } catch .unavailable {
            problem = .downloadsBlocked
        }
    }
}
```

## `Files`

```k
@Platform(side: .client)
public interface Files {
    func open(accepting types: List<FileType>) async throws FileOpenError -> File

    func openAll(accepting types: List<FileType>) async throws FileOpenError -> List<File>

    func save(_ file: File) throws FileSaveError
}
```

- **`open` shows the browser's file picker** for one file of the given types (any file when the list is empty) and resolves once the file is read. `openAll` allows several, answered in the order chosen.
- **A picker opens only in response to a user's action.** `open` must therefore be called from an event handler before its first `await`, as in `button { "Import" }.on(click: { transfer.importBoard() })`. The call shows the picker synchronously and awaits only the choice.
- **`save` starts a download** of the file's bytes under `file.name`, with its media type. It is synchronous: the browser takes over from there, and whether the user keeps the file isn't observable.
- **Client-only.** `Files` is a platform interface injectable only into services ([10-services.md](10-services.md#layering)), and a service that injects it is client-only ([10-services.md](10-services.md#serverclient-boundary)), so the server never touches it, by construction.
- The stdlib ships one production implementation, **`BrowserFiles`**, built on a hidden `<input type="file">` and an object URL. It is the only one, so `@Inject let files: Files` resolves to it with no `use` rule.

## `File` and `FileType`

```k
public struct File implements Equatable {
    let name: String
    let mediaType: String
    let bytes: Bytes

    static var maximumSize: Int   // 16 MiB, 16777216 bytes

    var text: String?             // the bytes as UTF-8, nil if they aren't valid UTF-8
    var size: Int                 // the number of bytes

    static func fromText(name: String, type: FileType, text: String) -> File
}
```

- **A `File` is a value.** Its content is read whole into `bytes` before `open` returns, so code that holds a `File` never waits or fails again, and a test can build one with `File.fromText(…)` or its memberwise initializer.
- **Typed content:** `bytes` for binary data, `text` for text (`nil` rather than a garbled string when the file isn't UTF-8), and `Json.decode(file.bytes)` straight into a `Codable` type ([08-types.md](08-types.md)). Decoding is the app's step, so its `DecodingError` is handled where the app knows what the file should hold.
- **`File.maximumSize` is 16 MiB.** A larger file is `.tooLarge(limit:)` without being read: reading it whole would hold it in memory. Large files need streaming, which is an [open question](#open-questions).
- `mediaType` is what the browser reports, empty when it doesn't know.

`FileType` names what the picker offers. Each case has a `mediaType` and a `fileExtension`, and the picker's `accept` lists both, so a `.json` filter works whether the system knows the extension or the media type:

| Case | Media type | Extension |
|---|---|---|
| `.json` | `application/json` | `json` |
| `.text` | `text/plain` | `txt` |
| `.csv` | `text/csv` | `csv` |
| `.markdown` | `text/markdown` | `md` |
| `.png`, `.jpeg`, `.svg` | `image/png`, `image/jpeg`, `image/svg+xml` | `png`, `jpg`, `svg` |
| `.pdf` | `application/pdf` | `pdf` |
| `.other(mediaType:fileExtension:)` | as given | as given |

The `accept` filter is a hint the user can override in most pickers, so the app still checks what it reads.

## Errors

```k
public enum FileOpenError implements Error, Equatable {
    case cancelled              // the picker was closed without a choice
    case tooLarge(limit: Int)   // a chosen file is over File.maximumSize
    case unreadable             // the browser couldn't read it, or can't show a picker
}

public enum FileSaveError implements Error, Equatable {
    case unavailable            // the browser refuses downloads here
}
```

- **`.cancelled` is an error, not `nil`,** so the result is always a file and a `catch .cancelled` that does nothing reads as the deliberate choice it is.
- Open and save fail in different ways, so each declares exactly what it throws, as local storage's reads and writes do.

## Tests: `TestFiles`

**`k test` installs `TestFiles`** (from `k.test`) for `Files` by default ([21-testing.md](21-testing.md#test-doubles-and-defaults)): no picker opens and no download starts. A test reaches it with `@Inject let files: TestFiles`:

| Member | Meaning |
|---|---|
| `choose(_ file: File)` | the user chooses `file` at the next `open` or `openAll`; several calls choose several files |
| `failOpen(error: FileOpenError)` | the next `open` or `openAll` throws `error` |
| `failSaves(error: FileSaveError)` | every following `save` throws `error` |
| `saved: List<File>` | every file saved so far |
| `requests: List<List<FileType>>` | the types of every `open` and `openAll` |

`open` with nothing chosen throws `.cancelled`, as a user closing the picker would.

```k
test BoardTransfer {
    @Inject let files: TestFiles
    @Inject let transfer: BoardTransfer

    test "an exported board imports back" {
        transfer.exportBoard()
        files.choose(files.saved[0])

        await transfer.importBoard()

        expect transfer.problem == nil
        expect files.requests == [[.json]]
    }
}
```

## Why

- **An injected interface** keeps file I/O where the rest of the platform's I/O is, testable with a hermetic double and checked client-only by the compiler.
- **Whole-file values** suit what web apps import and export (settings, documents, data sets, images under a few megabytes) and keep errors at one place: the `open`.
- **Typed errors** make the three ways an open ends (cancelled, too large, unreadable) explicit, and keep the save's one failure separate.

## Open questions

- **Streaming large files** (`Stream<Bytes>`) and a configurable size limit.
- **Saving in place** with the File System Access API where browsers have it (`save` always downloads today).
- **Files dragged onto the page** ([41-drag-and-drop.md](41-drag-and-drop.md#open-questions)) and pasted from the clipboard.
- **Older browsers without the picker's `cancel` event** never settle a cancelled `open`; the event is in every current engine.
