import k.device.File
import k.device.Files
import k.encoding.Json

/// Exports a board as a JSON file and imports one back
/// (`docs/design/42-files.md`).
///
/// An export is a download of a `BoardFile`, named after the board. An
/// import asks for a `.json` file, decodes it into a `BoardFile` and hands
/// the board to `BoardService`, which replaces the board with the same id
/// or adds it. Every failure is a `TransferProblem` in `problem`; closing
/// the picker isn't one.
public service BoardTransfer {
    @Inject let files: Files
    @Inject let boards: BoardService

    /// The format version this app writes, and the newest it reads.
    let format: Int = 1

    /// The latest failure, or `nil`.
    @State var problem: TransferProblem? = nil

    /// Downloads the board with `id` as `<its-title>.json`. Does nothing
    /// for an unknown board.
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

    /// Asks the user for a board file and imports it. Call it from a click
    /// handler, before any other `await`: browsers show a picker only
    /// then.
    ///
    /// - Returns: the imported board's id, or `nil` when nothing was
    ///   imported (the picker was closed, or `problem` says why).
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

    /// Hides the current problem.
    func dismissProblem() {
        problem = nil
    }

    /// Decodes a chosen file and imports its board.
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
