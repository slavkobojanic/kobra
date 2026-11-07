// A view may have several root nodes; each static root is its own template and they are returned
// in order (05-views.md).
component Cart {
    @State var quantity: Int = 1

    view {
        heading(level: 2) { "Cart" }

        text { "Total: {quantity}" }
    }
}
