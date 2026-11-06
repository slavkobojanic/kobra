// An optional prop passed both as a named argument and chained is set twice:
// K3104 at the chained name.
component Stepper {
    @Binding var value: Int
    @Prop let step: Int = 1

    view {
        text { "{value}" }
    }
}

component Cart {
    @State var quantity: Int = 1

    view {
        Stepper(value: bind quantity, step: 5)
            .step(2)
    }
}
