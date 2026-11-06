/// Lays out its content in a grid of equal columns.
@Native
public component Grid {
    /// The content.
    @Slot let content: View
    /// The number of columns.
    @Prop let columns: Int = 2
    /// The gap between cells.
    @Prop let gap: Spacing? = nil
}
