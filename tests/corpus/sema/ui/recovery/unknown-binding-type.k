// A `@Binding` with an unknown type is K1202; binding a place to it is not then a
// type mismatch.
component Stepper {
    @Binding var value: Nmber

    view {
        text { "{value}" }
    }
}

component Cart {
    @State var quantity: Int = 1

    view {
        Stepper(value: bind quantity)
    }
}
