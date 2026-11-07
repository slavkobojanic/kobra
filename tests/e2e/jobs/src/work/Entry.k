import k.crypto.Uuid

/// A line a job wrote.
public entity Entry {
    @Id let id: Uuid
    let text: String
}
