// A loop over a `@State` list binds fields of its items, as in the cart example of
// 05-views.md: `bind item.quantity`.
struct CartItem implements Equatable {
    let id: Int
    var quantity: Int
}

component Stepper {
    @Binding var value: Int

    view {
        text { "{value}" }
    }
}

component Cart {
    @State var items: List<CartItem> = []

    view {
        for item in items key item.id {
            Stepper(value: bind item.quantity)
        }
    }
}
