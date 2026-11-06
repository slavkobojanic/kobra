// sema.md §12.2: an optional prop may also be passed as a named argument.
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
    }
}
