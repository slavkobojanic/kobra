// A `@Prop` can't be bound: the child would gain write access to the parent's
// one-way input (K3015 at the operand).
component Stepper {
    @Binding var value: Int

    view {
        text { "{value}" }
    }
}

component Row {
    @Prop let quantity: Int

    view {
        Stepper(value: bind quantity)
    }
}
