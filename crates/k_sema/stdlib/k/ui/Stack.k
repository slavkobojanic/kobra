/// Lays out its content in a row or a column with a gap.
@Native
public component Stack {
    /// The content.
    @Slot let content: View
    /// The direction.
    @Prop let axis: Axis = .vertical
    /// The gap between items.
    @Prop let gap: Spacing? = nil
}
