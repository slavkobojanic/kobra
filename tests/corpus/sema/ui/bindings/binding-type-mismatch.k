// The bound place must have the binding's type: a `String` can't bind an
// `Int` binding (K2001 at the place).
component Stepper {
    @Binding var value: Int

    view {
        text { "{value}" }
    }
}

component Cart {
    @State var label: String = ""
    @State var flag: Int = 0

    view {
        Stepper(value: bind label)
        checkbox(isChecked: bind flag)
    }
}
