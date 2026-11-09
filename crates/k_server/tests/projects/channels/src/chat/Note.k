import k.crypto.Uuid

/// A posted message.
public entity Note {
    @Id let id: Uuid
    let text: String
}
