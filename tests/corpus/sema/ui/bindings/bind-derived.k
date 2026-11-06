// `@Derived` values can't be bound (04-components.md); `bind` of the `@State` it
// is computed from is fine.
component Stepper {
    @Binding var value: Int

    view {
        text { "{value}" }
    }
}

component Cart {
    @State var quantity: Int = 1
    @Derived let doubled: Int = quantity * 2

    view {
        Stepper(value: bind doubled)
        Stepper(value: bind quantity)
    }
}
