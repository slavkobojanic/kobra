// Literals and other non-places can't be bound.
component Stepper {
    @Binding var value: Int

    view {
        text { "{value}" }
    }
}

component Cart {
    @State var quantity: Int = 1

    view {
        Stepper(value: bind 5)
        Stepper(value: bind quantity + 1)
    }
}
