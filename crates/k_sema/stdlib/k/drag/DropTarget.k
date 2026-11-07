/// A region that accepts drags of one kind and fires `dropped` with their
/// payload (`docs/design/41-drag-and-drop.md`).
///
/// `accepts` refines the kind with a rule on the payload: a payload it
/// rejects can't be dropped here, with the pointer or the keyboard. While
/// an accepted drag is over the region it carries the `data-k-drop-target`
/// attribute and the `dropTarget` token's outline.
@Native
public component DropTarget<T> {
    /// The content of the region.
    @Slot let content: View
    /// The [`DragKind`] of the drags accepted.
    @Prop let kind: DragKind<T>
    /// The region's name, announced while a drag is over it ("Trash").
    @Prop let label: String
    /// The accept rule: `true` for a payload that may be dropped here.
    /// Every payload of the kind by default.
    @Prop let accepts: (T) -> Bool = { payload => true }
    /// Fires with the payload of an accepted drop.
    @Emits let dropped: Event<T>
}
