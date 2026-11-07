/// How far a board was at one moment: the progress chart plots these.
public struct ProgressPoint implements Codable, Equatable {
    /// When the board changed.
    let at: Date

    /// The cards in the board's last column then.
    let done: Int

    /// All of the board's cards then.
    let total: Int
}
