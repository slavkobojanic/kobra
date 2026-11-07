/// The content of an exported board file: the board and the version of the
/// file's format, so a later version of the app can read older files.
public struct BoardFile implements Codable, Equatable {
    /// The format version. `BoardTransfer` writes and reads version 1.
    let format: Int

    /// The board.
    let board: Board
}
