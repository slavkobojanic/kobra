import k.crypto.Uuid

public entity Note {
    @Id let id: Uuid
    var title: String
}
