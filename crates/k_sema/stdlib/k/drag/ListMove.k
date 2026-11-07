/// An item dropped on a [`ReorderableList`], as its `moved` event
/// delivers it (`docs/design/41-drag-and-drop.md`).
public struct ListMove<T> {
    /// The item that moved.
    let item: T
    /// The [`ReorderableList.id`] of the list it came from.
    let fromList: String
    /// Its position in that list.
    let fromIndex: Int
    /// The [`ReorderableList.id`] of the list it was dropped on.
    let toList: String
    /// Its position in that list, counted after removing it from its own
    /// list: remove it, then insert it there.
    let toIndex: Int
}
