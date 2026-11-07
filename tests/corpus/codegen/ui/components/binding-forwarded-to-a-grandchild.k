// A child passing its own `@Binding` on with `bind` forwards the pair unchanged instead of wrapping
// it (04-components.md).
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
