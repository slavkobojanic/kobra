/// Makes its content draggable with a pointer (mouse, pen or touch) and
/// with the keyboard, carrying a typed payload to any `DropTarget` of the
/// same kind (`docs/design/41-drag-and-drop.md`).
///
/// The content is focusable. Space or Enter picks it up, the arrow keys
/// choose a drop target, Space or Enter drops and Escape cancels, each step
/// announced in a live region. A touch drag starts after a short press, so
/// a swipe still scrolls.
@Native
public component Draggable<T> {
    /// The content.
    @Slot let content: View
    /// The kind of drag, which decides the drop targets that accept it.
    @Prop let kind: DragKind<T>
    /// The value a drop delivers.
    @Prop let payload: T
    /// The name announced for the dragged content ("Write tests").
    @Prop let label: String
}
