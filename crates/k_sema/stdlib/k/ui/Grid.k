/// Lays out its content in a grid of equal columns.
@Native
public component Grid {
    /// The cells, filled row by row.
    @Slot let content: View
    /// The number of columns.
    @Prop let columns: Int = 2
    /// The gap between cells, a [`k.core.Spacing`] token; none when `nil`.
    @Prop let gap: Spacing? = nil
}
