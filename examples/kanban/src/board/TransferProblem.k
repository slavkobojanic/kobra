/// Why an import or an export didn't happen.
public enum TransferProblem implements Equatable {
    /// The chosen file is larger than `limit` bytes.
    case tooLarge(limit: Int)
    /// The browser couldn't read the chosen file.
    case unreadable
    /// The file isn't an exported board: `detail` says where it differs.
    case notABoard(detail: String)
    /// The file is a board of a newer format version than this app reads.
    case newerFormat(version: Int)
    /// The browser refused to start the download.
    case downloadsBlocked

    /// What to tell the user.
    var message: String {
        return match self {
            case .tooLarge(let limit) { "That file is too large to be a board (over {limit} bytes)." }
            case .unreadable { "That file couldn't be read." }
            case .notABoard(let detail) { "That file isn't a Kanban board ({detail})." }
            case .newerFormat(let version) { "That board was exported by a newer Kanban (format {version})." }
            case .downloadsBlocked { "This browser blocked the download." }
        }
    }
}
