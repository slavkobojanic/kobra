// A literal passed to a `@Binding` misses `bind`, but inserting it would not
// help: K3016 without a fix.
component Stepper {
    @Binding var value: Int

    view {
        text { "{value}" }
    }
}

component Cart {
    view {
        Stepper(value: 1)
    }
}
