// Named and chained prop values are checked against the prop's type (K2001 at
// the value).
component Stepper {
    @Binding var value: Int
    @Prop let label: String
    @Prop let step: Int = 1

    view {
        text { "{label} {value}" }
    }
}

component Cart {
    @State var quantity: Int = 1

    view {
        Stepper(value: bind quantity, label: 42)
            .step("five")
        Stepper(value: bind quantity, label: "Qty")
            .step(5)
    }
}
