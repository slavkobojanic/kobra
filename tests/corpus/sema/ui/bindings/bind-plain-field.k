// A plain `var` field is not reactive, so it can't be bound; nor can a local.
component Stepper {
    @Binding var value: Int

    view {
        text { "{value}" }
    }
}

component Cart {
    var scratch: Int = 0

    view {
        let local = 3

        Stepper(value: bind scratch)
        Stepper(value: bind local)
    }
}
