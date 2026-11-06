// Setting the same option twice is a compile error (04-components.md), even with
// another option in between.
component Stepper {
    @Binding var value: Int
    @Prop let step: Int = 1
    @Prop let max: Int? = nil

    view {
        text { "{value}" }
    }
}

component Cart {
    @State var quantity: Int = 1

    view {
        Stepper(value: bind quantity)
            .step(5)
            .max(9)
            .step(2)
    }
}
