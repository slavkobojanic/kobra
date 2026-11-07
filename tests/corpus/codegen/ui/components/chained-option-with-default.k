// A prop with a default is set with a chained call; an option that isn't set is not passed, and the
// child evaluates its default itself (04-components.md, codegen.md §10).
component Stepper {
    @Binding var value: Int
    @Prop let step: Int = 1
    @Prop let max: Int? = nil

    view {
        text { "{value} by {step}" }
    }
}

component Cart {
    @State var small: Int = 1
    @State var large: Int = 10

    view {
        box {
            Stepper(value: bind small)

            Stepper(value: bind large)
                .step(5)
                .max(99)
        }
    }
}
