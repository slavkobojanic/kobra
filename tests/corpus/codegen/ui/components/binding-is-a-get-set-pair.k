// A `@Binding` prop is a `{ get, set }` pair; `bind quantity` creates it over the parent's signal,
// and the child's write goes through `set` (04-components.md, codegen.md §10).
component Stepper {
    @Binding var value: Int
    @Prop let step: Int = 1

    view {
        box {
            button { "-" }
                .on(click: { value -= step })

            text { "{value}" }

            button { "+" }
                .on(click: { value += step })
        }
    }
}

component Cart {
    @State var quantity: Int = 1

    view {
        Stepper(value: bind quantity)
    }
}
