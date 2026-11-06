// Required, optional and scoped slots.
component Card {
    @Prop let title: String
    @Prop let items: List<CartItem>
    @Slot let content: View
    @Slot let footer: View? = nil
    @Slot let row: (CartItem) -> View
    @Slot let header: ((String) -> View)?
}
