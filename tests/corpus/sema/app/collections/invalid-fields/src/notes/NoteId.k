import k.crypto.Uuid

/// A note's id.
public struct NoteId implements Codable, Equatable {
    let value: Uuid
}
