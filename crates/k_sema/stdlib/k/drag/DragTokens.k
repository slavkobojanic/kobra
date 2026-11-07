/// The theme tokens `k.drag` declares, with defaults. An app overrides
/// them with the assignment form in its own theme
/// (`dropIndicator = #16A34A`).
public theme DragTokens {
    /// The color of the line that shows where a dragged item will land in
    /// a [`ReorderableList`].
    color dropIndicator: Color = #2563EB
    /// The outline color of a [`DropTarget`] while an accepted drag is over
    /// it.
    color dropTarget: Color = #93C5FD
}
