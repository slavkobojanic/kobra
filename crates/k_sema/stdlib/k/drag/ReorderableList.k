/// A list whose items can be reordered, and moved to and from other
/// reorderable lists of the same kind, with a pointer, touch or the
/// keyboard (`docs/design/41-drag-and-drop.md`).
///
/// It renders a `list` named `label` with one `item` per element of
/// `items`, drawn by the chained `row` slot and kept by `key` like
/// `for … key`.
/// The list doesn't move anything itself: a drop fires `moved` on the
/// list that receives the item, and the app changes its state, which
/// redraws both lists.
///
/// Keyboard: Space or Enter on a focused item picks it up, ArrowUp and
/// ArrowDown choose its position, ArrowLeft and ArrowRight the previous or
/// next list of the same kind, Space or Enter drops and Escape cancels;
/// every step is announced in a live region.
@Native
public component ReorderableList<T> {
    /// Draws one item.
    @Slot let row: (T) -> View
    /// The kind of drag the list's items are, and accepts.
    @Prop let kind: DragKind<T>
    /// Identifies the list in a `ListMove` ("todo", a column's id).
    @Prop let id: String
    /// The items, in order.
    @Prop let items: List<T>
    /// Each item's identity: rows are kept by it, like `for … key`.
    @Prop let key: (T) -> String
    /// The list's accessible name, announced while an item is over it.
    @Prop let label: String
    /// Each item's name in announcements ("Write tests"); its key when the
    /// name is empty, as it is by default.
    @Prop let itemLabel: (T) -> String = { item => "" }
    /// The accept rule for items coming from another list. Every item of
    /// the kind by default.
    @Prop let accepts: (T) -> Bool = { item => true }
    /// Fires on the receiving list when an item is dropped on it.
    @Emits let moved: Event<ListMove<T>>
}
