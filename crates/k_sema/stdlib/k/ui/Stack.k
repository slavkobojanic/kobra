/// Lays out its content in a row or a column with a gap.
@Native
public component Stack {
    /// The items, in order.
    @Slot let content: View
    /// The direction of the items: a row for [`Axis.horizontal`], a column
    /// for [`Axis.vertical`].
    @Prop let axis: Axis = .vertical
    /// The gap between items, a [`k.core.Spacing`] token; none when `nil`.
    @Prop let gap: Spacing? = nil
}
