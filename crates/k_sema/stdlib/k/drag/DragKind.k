/// What can be dragged, paired with the type of its payload:
/// `DragKind<CardDrag> = DragKind(name: "card")`
/// (`docs/design/41-drag-and-drop.md`). A drop target or a reorderable
/// list accepts only drags of its own kind, so a drop handler always
/// receives the payload type it declares. The name is a string literal;
/// kinds with the same name are the same kind.
@Native
public struct DragKind<T> {
    /// Creates the kind named `name`.
    ///
    /// - Parameter name: a string literal; kinds with the same name are the
    ///   same kind.
    @Native
    init(name: String) {}

    /// The kind's name.
    var name: String
}
