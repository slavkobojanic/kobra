// `bind x` passes a `@State` field to a `@Binding` prop; the bound place resolves
// to the parent's field.
component Stepper {
    @Binding var value: Int

    view {
        button { "+" }
            .on(click: { value += 1 })
    }
}

component Cart {
    @State var quantity: Int = 1

    view {
        Stepper(value: bind quantity)
        //                  ^^^^^^^^ resolves: member corpus.Cart.quantity
        //                  ^^^^^^^^ type: Int
    }
}
