// A prop with a default, or an optional (`T?`) prop, is set with a generated
// chained method. Both are order-independent and may be omitted.
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
           //^^^^ resolves: member corpus.Stepper.step
            .max(99)
           //^^^ resolves: member corpus.Stepper.max
        Stepper(value: bind quantity)
            .max(10)
            .step(2)
        Stepper(value: bind quantity)
    }
}
