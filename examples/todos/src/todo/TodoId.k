import k.crypto.Uuid

/// Identifies a todo for its whole life, including across reloads.
///
/// A dedicated type, rather than a bare `Uuid`, means a todo id can't be
/// passed where some other kind of id is expected. Ids are random, so todos
/// created in two tabs never collide.
public struct TodoId implements Codable, Equatable, Hashable {
    /// The random UUID behind this id.
    let value: Uuid
}
