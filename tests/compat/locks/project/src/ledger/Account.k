import k.crypto.Uuid

/// One account (`schema.lock`).
public entity Account {
    @Id let id: Uuid
    @Unique let name: String
    let cents: Int
}
