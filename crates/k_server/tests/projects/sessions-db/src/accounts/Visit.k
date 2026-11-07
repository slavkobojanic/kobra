import k.crypto.Uuid

/// A recorded visit.
public entity Visit {
    @Id let id: Uuid
    let who: String
}
