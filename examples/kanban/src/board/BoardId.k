import k.crypto.Uuid

/// Identifies a board for its whole life, across reloads, exports and
/// imports. Its text is the board's URL segment (`/boards/<id>`).
public struct BoardId implements Codable, Equatable, Hashable {
    /// The random UUID behind this id.
    let value: Uuid
}
