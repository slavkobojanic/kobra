import k.crypto.Uuid

/// A label.
public entity Tag {
    @Id let id: Uuid

    var label: String
}
