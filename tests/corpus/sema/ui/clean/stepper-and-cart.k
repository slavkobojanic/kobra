// The two-way binding example of 04-components.md: `@Binding` in the child,
// `bind` at the call site, chained optional props.
component Stepper {
    @Binding var value: Int
    @Prop let step: Int = 1
    @Prop let max: Int? = nil

    view {
        box {
            button { "−" }
                .on(click: { value -= step })

            text { "{value}" }

            button { "+" }
                .on(click: { value += step })
        }
    }
}

component Cart {
    @State var quantity: Int = 1
    @Prop let price: Int

    view {
        Stepper(value: bind quantity)
            .step(5)
            .max(99)

        text { "Total: {quantity * price}" }
    }
}
