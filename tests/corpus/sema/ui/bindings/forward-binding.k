// A `@Binding` can be passed on to a grandchild with `bind`.
component Stepper {
    @Binding var value: Int

    view {
        text { "{value}" }
    }
}

component QuantityRow {
    @Binding var quantity: Int

    view {
        Stepper(value: bind quantity)
    }
}

component Cart {
    @State var quantity: Int = 1

    view {
        QuantityRow(quantity: bind quantity)
    }
}
